using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Xml;
using CodeWalker.GameFiles;

namespace CodeWalker.Bridge;

/// <summary>
/// Native-callable entry points wrapping CodeWalker.Core's ymap &lt;-&gt; xml conversion.
/// Not thread-safe: CodeWalker's JenkIndex/JenkHash caches are process-global statics,
/// so callers must invoke these functions from a single thread.
/// </summary>
public static unsafe class Exports
{
  private static string _lastError = string.Empty;

  /// <summary>Loads GTA V archive keys from the locally installed game.</summary>
  [UnmanagedCallersOnly]
  public static int LoadGameKeys(byte* gamePathUtf8) =>
    Try(() => {
      string gamePath = PtrToString(gamePathUtf8);
      string executable = Directory.EnumerateFiles(gamePath)
        .SingleOrDefault(file => Path.GetFileName(file).Equals("GTA5.exe", StringComparison.OrdinalIgnoreCase)) ??
        throw new FileNotFoundException("GTA5.exe is required (GTA V Legacy archives only).", gamePath);
      GTA5Keys.GenerateV2(File.ReadAllBytes(executable), _ => { });
      if (GTA5Keys.PC_AES_KEY == null) {
        throw new InvalidDataException("CodeWalker could not locate the archive key in GTA5.exe.");
      }
      var load = typeof(GTA5Keys).GetMethod("LoadFromPath") ??
        throw new MissingMethodException("GTA5Keys.LoadFromPath is unavailable.");
      var parameters = load.GetParameters();
      if (!parameters.Any(parameter => parameter.Name == "key")) {
        throw new NotSupportedException("This CodeWalker DLL cannot accept an executable-derived key; rebuild CodeWalker.Core.");
      }
      object[] arguments = parameters.Select(parameter => parameter.Name switch {
        "path" => (object)gamePath,
        "gen9" => false,
        "key" => Convert.ToBase64String(GTA5Keys.PC_AES_KEY),
        _ => throw new NotSupportedException($"Unknown key-loading parameter: {parameter.Name}"),
      }).ToArray();
      load.Invoke(null, arguments);
    });

  /// <summary>Extracts YMAPs, YBNs and dlclist.xml recursively, preserving archive provenance.</summary>
  [UnmanagedCallersOnly]
  public static int ExtractRpf(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => ExtractRpfFiles(PtrToString(inputPathUtf8), PtrToString(outputPathUtf8), null));

  /// <summary>Extracts only the selected nested archive subtree.</summary>
  [UnmanagedCallersOnly]
  public static int ExtractRpfSubtree(byte* inputPathUtf8, byte* subtreeUtf8, byte* outputPathUtf8) =>
    Try(() => ExtractRpfFiles(PtrToString(inputPathUtf8), PtrToString(outputPathUtf8), PtrToString(subtreeUtf8)));

  /// <summary>Lists virtual paths of all nested archives without extracting their contents.</summary>
  [UnmanagedCallersOnly]
  public static int ListRpfPaths(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => {
      List<string> paths = new();
      void Visit(RpfFile archive) {
        paths.Add(archive.Path.Replace('\\', '/'));
        foreach (RpfFile child in archive.Children) {
          Visit(child);
        }
      }
      Visit(ScanRpf(PtrToString(inputPathUtf8)));
      File.WriteAllText(PtrToString(outputPathUtf8), JsonSerializer.Serialize(paths));
    });

  private static RpfFile ScanRpf(string inputPath)
  {
    RpfFile root = new(inputPath, Path.GetFileName(inputPath));
    List<string> errors = new();
    root.ScanStructure(_ => { }, errors.Add);
    if (errors.Count != 0) {
      throw new InvalidDataException(string.Join(Environment.NewLine, errors));
    }
    return root;
  }

  private static void ExtractRpfFiles(string inputPath, string outputPath, string subtree)
  {
      Directory.CreateDirectory(outputPath);
      RpfFile root = ScanRpf(inputPath);
      List<object> files = new();
      SortedSet<string> rpfNames = new(StringComparer.Ordinal);
      void Extract(RpfFile archive) {
        foreach (RpfFileEntry entry in archive.AllEntries.OfType<RpfFileEntry>()) {
          string entryPath = entry.Path.Replace('\\', '/');
          if (subtree != null && !entryPath.StartsWith(subtree + "/", StringComparison.OrdinalIgnoreCase)) {
            continue;
          }
          int extension = entry.Name.LastIndexOf('.');
          string name = extension > 0 ? entry.Name[..extension] : entry.Name;
          if (!string.IsNullOrWhiteSpace(name)) {
            rpfNames.Add(name);
            rpfNames.Add(name.ToLowerInvariant());
          }
          if (!entry.NameLower.EndsWith(".ymap") && !entry.NameLower.EndsWith(".ybn") &&
              !entryPath.EndsWith("/common/data/dlclist.xml")) {
            continue;
          }
          byte[] data = archive.ExtractFile(entry) ??
            throw new InvalidDataException($"Could not extract {entry.Path}: {archive.LastError}");
          if (entry is RpfResourceFileEntry resource) {
            data = ResourceBuilder.AddResourceHeader(resource, ResourceBuilder.Compress(data));
          }
          string stored = $"{files.Count:D8}{Path.GetExtension(entry.NameLower)}";
          File.WriteAllBytes(Path.Combine(outputPath, stored), data);
          string sha256 = Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();
          files.Add(new { name = entry.NameLower, source = entry.Path.Replace('\\', '/'), stored, sha256 });
        }
        foreach (RpfFile child in archive.Children) {
          Extract(child);
        }
      }
      Extract(root);
      File.WriteAllText(Path.Combine(outputPath, "files.json"), JsonSerializer.Serialize(files));
        File.WriteAllText(Path.Combine(outputPath, "rpf_names.json"), JsonSerializer.Serialize(rpfNames));
  }

  [UnmanagedCallersOnly]
  public static int GetLastError(byte* buffer, int bufferSize)
  {
    byte[] bytes = Encoding.UTF8.GetBytes(_lastError);
    int count = Math.Min(bytes.Length, Math.Max(bufferSize - 1, 0));
    if (bufferSize > 0) {
      Marshal.Copy(bytes, 0, (IntPtr)buffer, count);
      buffer[count] = 0;
    }
    return bytes.Length;
  }

  // Loads a .ymap file and discards it, only to register its Strings into the
  // global JenkIndex so that later exports can resolve hash references by name.
  [UnmanagedCallersOnly]
  public static int PreloadNames(byte* inputPathUtf8) =>
    Try(() => {
      string inputPath = PtrToString(inputPathUtf8);
      byte[] data = File.ReadAllBytes(inputPath);
      if (data.AsSpan().StartsWith("PSIN"u8)) {
        PsoFile pso = new();
        pso.Load(data);
        return;
      }
      YmapFile ymap = new();
      ymap.Load(data);
    });

  [UnmanagedCallersOnly]
  public static int YmapToXml(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => {
      string inputPath = PtrToString(inputPathUtf8);
      string outputPath = PtrToString(outputPathUtf8);
      byte[] data = File.ReadAllBytes(inputPath);
      string xml;
      if (data.AsSpan().StartsWith("PSIN"u8)) {
        PsoFile pso = new();
        pso.Load(data);
        xml = PsoXml.GetXml(pso);
      } else {
        YmapFile ymap = new();
        ymap.Load(data);
        xml = MetaXml.GetXml(ymap, out _);
      }
      File.WriteAllText(outputPath, xml, new UTF8Encoding(false));
    });

  [UnmanagedCallersOnly]
  public static int XmlToYmap(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => {
      string inputPath = PtrToString(inputPathUtf8);
      string outputPath = PtrToString(outputPathUtf8);
      XmlDocument doc = new();
      doc.Load(inputPath);
      byte[] data = XmlMeta.GetData(doc, MetaFormat.RSC, string.Empty) ??
        throw new InvalidOperationException($"XmlMeta.GetData returned null for '{inputPath}'");
      File.WriteAllBytes(outputPath, data);
    });

  [UnmanagedCallersOnly]
  public static int GameFileToXml(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => {
      string inputPath = PtrToString(inputPathUtf8);
      string outputPath = PtrToString(outputPathUtf8);
      byte[] data = File.ReadAllBytes(inputPath);
      string xml = Path.GetExtension(inputPath).ToLowerInvariant() switch {
        ".ybn" => ExportYbn(data),
        ".ynd" => ExportYnd(data),
        ".ymt" => ExportYmt(data),
        ".ytyp" => ExportYtyp(data),
        var extension => throw new NotSupportedException($"Unsupported game-file extension '{extension}'"),
      };
      File.WriteAllText(outputPath, xml, new UTF8Encoding(false));
    });

  [UnmanagedCallersOnly]
  public static int GameFileFromXml(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => {
      string inputPath = PtrToString(inputPathUtf8);
      string outputPath = PtrToString(outputPathUtf8);
      XmlDocument doc = new();
      doc.Load(inputPath);
      MetaFormat format = Path.GetExtension(outputPath).ToLowerInvariant() switch {
        ".ybn" => MetaFormat.Ybn,
        ".ynd" => MetaFormat.Ynd,
        ".ymt" or ".ytyp" => MetaFormat.RSC,
        var extension => throw new NotSupportedException($"Unsupported game-file extension '{extension}'"),
      };
      byte[] data = XmlMeta.GetData(doc, format, inputPath) ??
        throw new InvalidOperationException($"XmlMeta.GetData returned null for '{inputPath}'");
      File.WriteAllBytes(outputPath, data);
    });

  private static string ExportYbn(byte[] data)
  {
    YbnFile file = new();
    file.Load(data);
    return MetaXml.GetXml(file, out _);
  }

  private static string ExportYnd(byte[] data)
  {
    YndFile file = new();
    file.Load(data);
    return MetaXml.GetXml(file, out _);
  }

  private static string ExportYmt(byte[] data)
  {
    YmtFile file = new();
    file.Load(data);
    if (file.Meta != null) {
      foreach (string name in MetaTypes.GetStrings(file.Meta) ?? Array.Empty<string>()) {
        JenkIndex.Ensure(name);
      }
      return MetaXml.GetXml(file.Meta);
    }
    return MetaXml.GetXml(file, out _);
  }

  private static string ExportYtyp(byte[] data)
  {
    YtypFile file = new();
    file.Load(data);
    return file.Meta != null ? MetaXml.GetXml(file.Meta) : MetaXml.GetXml(file, out _);
  }

  private static int Try(Action action)
  {
    try {
      action();
      return 0;
    } catch (Exception ex) {
      _lastError = ex.ToString();
      return 1;
    }
  }

  private static string PtrToString(byte* utf8) => utf8 == null ? null : Marshal.PtrToStringUTF8((IntPtr)utf8);
}
