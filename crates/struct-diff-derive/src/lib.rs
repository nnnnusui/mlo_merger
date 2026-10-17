//! `#[derive(Diff)]` for `struct-diff`; see that crate for the attributes.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::{
  Attribute, Data, DeriveInput, Expr, Fields, Ident, Meta, Path, Token, Type, parse_macro_input,
  punctuated::Punctuated, spanned::Spanned,
};

#[proc_macro_derive(Diff, attributes(diff))]
pub fn derive_diff(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  expand(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// `#[diff(...)]` on the struct.
#[derive(Default)]
struct ContainerAttrs {
  name: Option<Ident>,
  context: Option<Type>,
  attrs: Vec<Meta>,
  field_attrs: Vec<Meta>,
}

/// How a field is compared.
enum Strategy {
  Replace,
  Nested,
  Threshold(Expr),
  With(Path),
}

/// `#[diff(...)]` on a field.
struct FieldAttrs {
  skip: bool,
  strategy: Strategy,
  context: Option<Expr>,
  output: Option<Type>,
  attrs: Vec<Meta>,
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
  let data = match &input.data {
    Data::Struct(data) => data,
    Data::Enum(data) => return Err(not_supported(data.enum_token.span)),
    Data::Union(data) => return Err(not_supported(data.union_token.span)),
  };
  let Fields::Named(fields) = &data.fields else {
    return Err(not_supported(data.fields.span()));
  };
  let container = parse_container_attrs(&input.attrs)?;
  let ident = &input.ident;
  let vis = &input.vis;
  let diff_ident = container.name.clone().unwrap_or_else(|| format_ident!("{ident}Diff"));
  let context = container.context.clone().unwrap_or_else(|| syn::parse_quote!(()));
  let type_params: Vec<&Ident> = input.generics.type_params().map(|param| &param.ident).collect();
  let mut generics = input.generics.clone();
  let serde_renames = serde_attrs(&input.attrs, "rename_all")?;
  let struct_attrs = &container.attrs;

  let mut diff_fields = Vec::new();
  let mut diff_values = Vec::new();
  let mut applies = Vec::new();
  let mut names = Vec::new();
  for field in &fields.named {
    let attrs = parse_field_attrs(&field.attrs)?;
    if attrs.skip {
      continue;
    }
    let name = field.ident.as_ref().unwrap();
    let field_vis = &field.vis;
    let ty = &field.ty;
    let span = ty.span();
    let module = match &attrs.strategy {
      Strategy::Replace => quote!(::struct_diff::replace),
      Strategy::Nested => quote!(::struct_diff::nested),
      Strategy::Threshold(_) => quote!(::struct_diff::threshold),
      Strategy::With(path) => quote!(#path),
    };
    let output = match (&attrs.strategy, &attrs.output) {
      (_, Some(output)) => quote!(#output),
      (Strategy::Nested, None) => quote_spanned!(span=> <#ty as ::struct_diff::Diff>::Output),
      _ => quote!(#ty),
    };
    let ctx = match (&attrs.strategy, &attrs.context) {
      (Strategy::Threshold(tolerance), _) => quote!(&(#tolerance)),
      (_, Some(context)) => quote!(&(#context)),
      (_, None) => quote!(ctx),
    };
    if mentions_any(ty, &type_params) {
      let bound = match &attrs.strategy {
        Strategy::Replace => {
          Some(quote!(::core::cmp::PartialEq + ::core::clone::Clone + ::core::fmt::Debug))
        }
        Strategy::Nested => Some(quote!(::struct_diff::Diff)),
        Strategy::Threshold(_) => {
          Some(quote!(::struct_diff::threshold::Threshold + ::core::fmt::Debug))
        }
        // The module's own signatures state what it needs.
        Strategy::With(_) => None,
      };
      if let Some(bound) = bound {
        generics.make_where_clause().predicates.push(syn::parse_quote!(#ty: #bound));
      }
    }
    let renames = serde_attrs(&field.attrs, "rename")?;
    let forwarded = container.field_attrs.iter().chain(&attrs.attrs);
    diff_fields.push(quote! {
      // Spelled as tools such as specta recognise it, to mark the field optional.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      #(#renames)*
      #(#[#forwarded])*
      #field_vis #name: ::core::option::Option<#output>
    });
    diff_values.push(quote_spanned! {span=>
      #name: #module::diff(&self.#name, &other.#name, #ctx)
    });
    let label = name.to_string();
    applies.push(quote_spanned! {span=>
      if let ::core::option::Option::Some(change) = &diff.#name {
        #module::apply(&mut self.#name, change).map_err(|error| error.at(#label))?;
      }
    });
    names.push(name);
  }

  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
  Ok(quote! {
    // No `#[doc]`: derives that register `doc` as a helper attribute, such as specta's, make
    // it ambiguous.
    #[derive(
      ::core::fmt::Debug,
      ::core::clone::Clone,
      ::core::default::Default,
      ::core::cmp::PartialEq,
      ::struct_diff::__private::serde::Serialize,
      ::struct_diff::__private::serde::Deserialize,
    )]
    #[serde(crate = "::struct_diff::__private::serde")]
    #(#serde_renames)*
    #(#[#struct_attrs])*
    #vis struct #diff_ident #impl_generics #where_clause {
      #(#diff_fields,)*
    }

    impl #impl_generics ::struct_diff::Diff for #ident #ty_generics #where_clause {
      type Context = #context;
      type Output = #diff_ident #ty_generics;

      #[allow(unused_variables)]
      fn diff(
        &self,
        other: &Self,
        ctx: &Self::Context,
      ) -> ::core::option::Option<Self::Output> {
        let diff = #diff_ident {
          #(#diff_values,)*
        };
        if true #(&& diff.#names.is_none())* {
          ::core::option::Option::None
        } else {
          ::core::option::Option::Some(diff)
        }
      }

      #[allow(unused_variables)]
      fn apply(
        &mut self,
        diff: &Self::Output,
      ) -> ::core::result::Result<(), ::struct_diff::ApplyError> {
        #(#applies)*
        ::core::result::Result::Ok(())
      }
    }
  })
}

fn not_supported(span: Span) -> syn::Error {
  syn::Error::new(span, "`Diff` can only be derived for structs with named fields")
}

fn parse_container_attrs(attrs: &[Attribute]) -> syn::Result<ContainerAttrs> {
  let mut parsed = ContainerAttrs::default();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("diff")) {
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("name") {
        parsed.name = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("context") {
        parsed.context = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("attr") {
        parsed.attrs.extend(parse_metas(&meta)?);
      } else if meta.path.is_ident("field_attr") {
        parsed.field_attrs.extend(parse_metas(&meta)?);
      } else {
        return Err(meta.error(
          "unknown `diff` struct attribute; expected `name`, `context`, `attr` or `field_attr`",
        ));
      }
      Ok(())
    })?;
  }
  Ok(parsed)
}

fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
  let mut skip = None;
  let mut strategies = Vec::new();
  let mut context = None;
  let mut output = None;
  let mut forwarded = Vec::new();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("diff")) {
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("skip") {
        skip = Some(meta.path.span());
      } else if meta.path.is_ident("nested") {
        strategies.push((meta.path.span(), Strategy::Nested));
      } else if meta.path.is_ident("threshold") {
        strategies.push((meta.path.span(), Strategy::Threshold(meta.value()?.parse()?)));
      } else if meta.path.is_ident("with") {
        strategies.push((meta.path.span(), Strategy::With(meta.value()?.parse()?)));
      } else if meta.path.is_ident("context") {
        context = Some((meta.path.span(), meta.value()?.parse::<Expr>()?));
      } else if meta.path.is_ident("output") {
        output = Some((meta.path.span(), meta.value()?.parse::<Type>()?));
      } else if meta.path.is_ident("attr") {
        forwarded.extend(parse_metas(&meta)?);
      } else {
        return Err(meta.error(
          "unknown `diff` field attribute; expected `skip`, `nested`, `threshold`, `with`, \
           `context`, `output` or `attr`",
        ));
      }
      Ok(())
    })?;
  }
  if let [_, (span, _), ..] = strategies.as_slice() {
    return Err(syn::Error::new(
      *span,
      "a field takes at most one of `nested`, `threshold` and `with`",
    ));
  }
  let strategy = strategies.pop().map_or(Strategy::Replace, |(_, strategy)| strategy);
  if let Some(span) = skip
    && (!matches!(strategy, Strategy::Replace) || context.is_some() || output.is_some())
  {
    return Err(syn::Error::new(span, "a skipped field takes no other `diff` attribute"));
  }
  match (&strategy, &context, &output) {
    (Strategy::Replace | Strategy::Threshold(_), Some((span, _)), _) => {
      return Err(syn::Error::new(*span, "`context` requires `nested` or `with`"));
    }
    (Strategy::Replace | Strategy::Nested | Strategy::Threshold(_), _, Some((span, _))) => {
      return Err(syn::Error::new(*span, "`output` requires `with`"));
    }
    _ => {}
  }
  Ok(FieldAttrs {
    skip: skip.is_some(),
    strategy,
    context: context.map(|(_, context)| context),
    output: output.map(|(_, output)| output),
    attrs: forwarded,
  })
}

/// The attributes listed in `attr(...)`.
fn parse_metas(meta: &syn::meta::ParseNestedMeta) -> syn::Result<Punctuated<Meta, Token![,]>> {
  let content;
  syn::parenthesized!(content in meta.input);
  Punctuated::parse_terminated(&content)
}

/// `#[serde(key = ...)]` attributes among `attrs`, to carry over to the generated struct.
fn serde_attrs(
  attrs: &[Attribute],
  key: &str,
) -> syn::Result<Vec<TokenStream>> {
  let mut found = Vec::new();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
    let Meta::List(list) = &attr.meta else {
      continue;
    };
    let nested = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
    for meta in nested.iter().filter(|meta| meta.path().is_ident(key)) {
      found.push(quote!(#[serde(#meta)]));
    }
  }
  Ok(found)
}

/// Whether `ty` names any of `params`.
fn mentions_any(
  ty: &Type,
  params: &[&Ident],
) -> bool {
  fn walk(
    tokens: TokenStream,
    params: &[&Ident],
  ) -> bool {
    tokens.into_iter().any(|token| match token {
      proc_macro2::TokenTree::Ident(ident) => params.contains(&&ident),
      proc_macro2::TokenTree::Group(group) => walk(group.stream(), params),
      _ => false,
    })
  }
  !params.is_empty() && walk(quote!(#ty), params)
}
