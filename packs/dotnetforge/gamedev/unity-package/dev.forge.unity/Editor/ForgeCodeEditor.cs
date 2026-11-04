// Forge as Unity's external script editor (Preferences > External Tools > External Script
// Editor). Double-clicked scripts open in Forge at their line: through the bridge when Forge
// is connected, else by starting Forge with "<file>:<line>:<column>".

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using Unity.CodeEditor;
using UnityEditor;
using UnityEngine;

namespace Forge.Unity.Editor
{
    [InitializeOnLoad]
    public class ForgeCodeEditor : IExternalCodeEditor
    {
        static ForgeCodeEditor()
        {
            CodeEditor.Register(new ForgeCodeEditor());
        }

        public CodeEditor.Installation[] Installations
        {
            get
            {
                return Candidates()
                    .Where(p => File.Exists(p) || Directory.Exists(p))
                    .Distinct()
                    .Select(p => new CodeEditor.Installation { Name = "Forge", Path = p })
                    .ToArray();
            }
        }

        public bool TryGetInstallationForPath(string editorPath, out CodeEditor.Installation installation)
        {
            if (IsForge(editorPath))
            {
                installation = new CodeEditor.Installation { Name = "Forge", Path = editorPath };
                return true;
            }
            installation = default;
            return false;
        }

        public void OnGUI()
        {
            EditorGUILayout.LabelField(ForgeBridge.IsConnected ? "Connected to Forge" : "Forge is not connected", EditorStyles.miniLabel);
            if (GUILayout.Button("Regenerate project files", GUILayout.Width(200)))
            {
                ForgeProjectFiles.Sync(out var message);
                UnityEngine.Debug.Log("Forge: " + message);
            }
        }

        public void SyncIfNeeded(string[] addedFiles, string[] deletedFiles, string[] movedFiles, string[] movedFromFiles, string[] importedFiles)
        {
            if (ForgeProjectFiles.Affects(addedFiles, deletedFiles, movedFiles, movedFromFiles))
                ForgeProjectFiles.Sync(out _);
        }

        public void SyncAll()
        {
            ForgeProjectFiles.Sync(out _);
        }

        public void Initialize(string editorInstallationPath)
        {
        }

        public bool OpenProject(string filePath = "", int line = -1, int column = -1)
        {
            if (!string.IsNullOrEmpty(filePath) && !ForgeProjectFiles.IsCodeAsset(filePath))
                return false;
            ForgeProjectFiles.SyncIfMissing();
            var root = Path.GetDirectoryName(Application.dataPath);
            var full = string.IsNullOrEmpty(filePath) ? root : Path.GetFullPath(filePath);
            if (!string.IsNullOrEmpty(filePath) && ForgeBridge.OpenFile(full, line, column))
                return true;
            var target = string.IsNullOrEmpty(filePath)
                ? full
                : full + ":" + Math.Max(line, 1) + ":" + Math.Max(column, 1);
            return Launch(CodeEditor.CurrentEditorInstallation, root, target);
        }

        static bool Launch(string forge, string root, string target)
        {
            try
            {
                ProcessStartInfo info;
                if (forge.EndsWith(".app", StringComparison.OrdinalIgnoreCase))
                    info = new ProcessStartInfo("open", "-a " + Quote(forge) + " --args " + Quote(root) + " " + Quote(target));
                else
                    info = new ProcessStartInfo(forge, Quote(root) + " " + Quote(target));
                info.UseShellExecute = false;
                Process.Start(info);
                return true;
            }
            catch (Exception e)
            {
                UnityEngine.Debug.LogWarning("Forge could not be started: " + e.Message);
                return false;
            }
        }

        static string Quote(string s)
        {
            return "\"" + s.Replace("\"", "\\\"") + "\"";
        }

        static bool IsForge(string path)
        {
            if (string.IsNullOrEmpty(path))
                return false;
            var name = Path.GetFileName(path.TrimEnd('/', '\\')).ToLowerInvariant();
            return name == "forge" || name == "forge.exe" || name == "forge.app"
                || (name.StartsWith("forge_") && name.EndsWith(".appimage"));
        }

        static IEnumerable<string> Candidates()
        {
            var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            switch (Application.platform)
            {
                case RuntimePlatform.WindowsEditor:
                    var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                    var programs = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
                    yield return Path.Combine(local, "Forge", "Forge.exe");
                    yield return Path.Combine(local, "Programs", "Forge", "Forge.exe");
                    yield return Path.Combine(programs, "Forge", "Forge.exe");
                    break;
                case RuntimePlatform.OSXEditor:
                    yield return "/Applications/Forge.app";
                    yield return Path.Combine(home, "Applications", "Forge.app");
                    break;
                default:
                    yield return "/usr/bin/forge";
                    yield return "/usr/local/bin/forge";
                    yield return Path.Combine(home, ".local", "bin", "forge");
                    foreach (var dir in new[] { Path.Combine(home, "Applications"), Path.Combine(home, "Downloads") })
                    {
                        if (!Directory.Exists(dir))
                            continue;
                        foreach (var f in Directory.GetFiles(dir, "Forge_*.AppImage"))
                            yield return f;
                    }
                    break;
            }
            var custom = Environment.GetEnvironmentVariable("FORGE_PATH");
            if (!string.IsNullOrEmpty(custom))
                yield return custom;
        }
    }
}
