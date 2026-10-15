using SharpCompress.Readers;

namespace VoiceInk.Core.Transcription.Local;

internal static class Archives
{
    /// <summary>
    /// Extracts a <c>.tar.bz2</c> into <paramref name="targetDir"/>. The archive's single top-level folder is
    /// stripped, entries that would escape the target are rejected, and the folder appears only when complete.
    /// </summary>
    public static void ExtractTarBz2(string archivePath, string targetDir)
    {
        var staging = targetDir + ".extracting";
        if (Directory.Exists(staging)) Directory.Delete(staging, recursive: true);
        Directory.CreateDirectory(staging);
        var root = Path.GetFullPath(staging) + Path.DirectorySeparatorChar;
        try
        {
            using (var stream = File.OpenRead(archivePath))
            using (var reader = ReaderFactory.OpenReader(stream))
            {
                while (reader.MoveToNextEntry())
                {
                    if (reader.Entry.IsDirectory || reader.Entry.Key is not { } key) continue;
                    var relative = StripTopLevel(key);
                    if (relative.Length == 0) continue;

                    var destination = Path.GetFullPath(Path.Combine(staging, relative));
                    if (!destination.StartsWith(root, StringComparison.Ordinal))
                    {
                        throw new InvalidDataException($"archive entry escapes the target folder: {key}");
                    }

                    Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                    using var entry = reader.OpenEntryStream();
                    using var file = File.Create(destination);
                    entry.CopyTo(file);
                }
            }

            if (Directory.Exists(targetDir)) Directory.Delete(targetDir, recursive: true);
            Directory.Move(staging, targetDir);
        }
        catch
        {
            if (Directory.Exists(staging)) Directory.Delete(staging, recursive: true);
            throw;
        }
    }

    private static string StripTopLevel(string key)
    {
        var parts = key.Replace('\\', '/').Split('/', StringSplitOptions.RemoveEmptyEntries);
        return parts.Length <= 1 ? "" : string.Join('/', parts[1..]);
    }
}
