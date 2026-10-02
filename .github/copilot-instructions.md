# Copilot Instructions (Meta-Guidelines)

## Project Management Philosophy

This document contains meta-guidelines that cannot be expressed through automated formatters or linters. For code style rules that can be enforced automatically (indentation, line length, etc.), use `rustfmt.toml` instead.

## Core Directives

### Language & Framework
- **Primary Language**: Rust (stable)
- **Target Platforms**: Windows and Linux
- **Development Environment**: Ubuntu via VSCode .devcontainer

### Documentation Requirements

1. **Code Documentation**
   - All public APIs MUST have doc comments (`///`)
   - Doc comments SHOULD be in English
   - Include examples in doc comments for non-trivial functions
   - Private functions SHOULD have doc comments for complex logic

2. **Project Documentation**
   - `docs/ARCHITECTURE.md`: Application structure, flow, and design decisions
   - `docs/TODO.md`: Project task tracking and roadmap
   - This file (`.github/copilot-instructions.md`): Meta-guidelines

3. **Documentation Updates**
   - Update `docs/TODO.md` when tasks change or complete
   - Update `docs/ARCHITECTURE.md` when structure or design changes
   - Update this file when new meta-guidelines emerge

## Version Control

- Commit messages in English
- Follow conventional commits format
- Keep commits atomic and focused

## Git

### When Committing

#### Message Format

```text
(?:[emoji-prefix]) [overview-message]

Created with AI ([Model Name or *]) in [Platform Name]

[detailed-messages]
```

#### Message Examples

```text
:hammer_and_wrench: Fix type errors in `/script/plopfile.ts`

Created with AI (Claude 3.5 Sonnet) in Cursor
```

```text
Ensure `Wve#partial()` infers undefineable type when the target is a record

Created with AI (GPT-4.1) in GitHub Copilot
```

```text
Add note kind editing feature

Created with AI (Claude 3.5 Sonnet) in Cursor

- Add `<EditNote/>` component for note editing functionality
- Implement note kind selection with visual feedback
- Add lane selection for note placement
```

#### Message Rules

- Write commit messages in English.
- **Emoji-prefix usage:**
  - **No prefix** for changes in `src/` that add new features to main code (excluding stories and spec files).
  - If a valid emoji-prefix is defined in `.gitmessage`, add it at the beginning of the first line.
- Use only one emoji-prefix.
- Use the emoji-prefix as a placeholder in the `:emoji:` format.
- Keep the first line concise whenever possible.
- Insert an empty line as the second line.
- Write the third line in the exact format: `Created with AI ([Model Name or *]) in [Platform Name]`.
- Use `*` as the model name when the underlying AI model name is not available.
- From the fourth line onward, add details or notes only when needed.
- Omit lines from the fourth line onward when the change is self-explanatory.
- Use technically accurate wording.
- Show Japanese translations of commit messages only in prompts, never in the commit message body itself.
- Do not include Japanese text in the commit message body.
