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
      YmapFile ymap = new();
      ymap.Load(File.ReadAllBytes(inputPath));
    });

  [UnmanagedCallersOnly]
  public static int YmapToXml(byte* inputPathUtf8, byte* outputPathUtf8) =>
    Try(() => {
      string inputPath = PtrToString(inputPathUtf8);
      string outputPath = PtrToString(outputPathUtf8);
      YmapFile ymap = new();
      ymap.Load(File.ReadAllBytes(inputPath));
      string xml = MetaXml.GetXml(ymap, out _);
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
