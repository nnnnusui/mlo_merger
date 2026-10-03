using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
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
