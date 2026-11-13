// Forge Editor Bridge: the Unity side of Forge's localhost protocol (protocol 1).
// Spec: docs/guide/developer/unity-bridge-protocol.md in the Forge repository.
//
// The editor listens on 127.0.0.1 (a free port) and writes Library/ForgeBridge.json with the
// port and a random token. Forge connects, sends {"type":"hello","token":...} and then
// commands (play, pause, resume, stop, step, refresh, syncSolution); the editor streams its
// state, console messages and "open file" requests. One JSON object per line.

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using UnityEditor;
using UnityEditor.Compilation;
using UnityEngine;

namespace Forge.Unity.Editor
{
    [InitializeOnLoad]
    public static class ForgeBridge
    {
        public const int Protocol = 1;
        const int MaxMessage = 1 << 20;
        const string InstanceFile = "Library/ForgeBridge.json";

        static TcpListener listener;
        static Thread acceptThread;
        static readonly List<Client> clients = new List<Client>();
        static readonly object clientsLock = new object();
        static readonly ConcurrentQueue<Action> mainThread = new ConcurrentQueue<Action>();
        static string token;
        static volatile string playMode = "edit";
        static volatile bool compiling;
        static string projectPath;
        static int pid;

        static ForgeBridge()
        {
            // Batch runs (tests, project file generation, CI) need no bridge.
            if (Application.isBatchMode)
                return;
            projectPath = Path.GetDirectoryName(Application.dataPath);
            pid = System.Diagnostics.Process.GetCurrentProcess().Id;
            playMode = CurrentPlayMode();
            try
            {
                Start();
            }
            catch (Exception e)
            {
                Debug.LogWarning("Forge bridge could not start: " + e.Message);
                return;
            }
            EditorApplication.update += Pump;
            EditorApplication.playModeStateChanged += _ => OnStateChanged();
            EditorApplication.pauseStateChanged += _ => OnStateChanged();
            CompilationPipeline.compilationStarted += _ => { compiling = true; Broadcast(StateMessage()); };
            CompilationPipeline.compilationFinished += _ => { compiling = false; Broadcast(StateMessage()); };
            Application.logMessageReceivedThreaded += OnLog;
            // Scripts reload in a new domain: the old listener must go first.
            AssemblyReloadEvents.beforeAssemblyReload += Stop;
            EditorApplication.quitting += Stop;
        }

        /// <summary>Whether Forge is connected.</summary>
        public static bool IsConnected
        {
            get { lock (clientsLock) { return clients.Exists(c => c.Authenticated); } }
        }

        /// <summary>Ask the connected Forge to open a file; false when none is connected.</summary>
        public static bool OpenFile(string path, int line, int column)
        {
            if (!IsConnected)
                return false;
            Broadcast(JsonUtility.ToJson(new OpenFileMessage
            {
                type = "openFile",
                path = path.Replace('\\', '/'),
                line = Math.Max(line, 1),
                column = Math.Max(column, 1),
            }));
            return true;
        }

        static void Start()
        {
            token = NewToken();
            listener = new TcpListener(IPAddress.Loopback, 0);
            listener.Start();
            var port = ((IPEndPoint)listener.LocalEndpoint).Port;
            var info = new InstanceInfo
            {
                port = port,
                pid = pid,
                token = token,
                protocol = Protocol,
                unityVersion = Application.unityVersion,
                projectPath = projectPath.Replace('\\', '/'),
            };
            File.WriteAllText(Path.Combine(projectPath, InstanceFile), JsonUtility.ToJson(info, true));
            acceptThread = new Thread(AcceptLoop) { IsBackground = true, Name = "Forge bridge" };
            acceptThread.Start();
        }

        static void Stop()
        {
            var l = listener;
            listener = null;
            try { l?.Stop(); } catch (Exception) { }
            lock (clientsLock)
            {
                foreach (var c in clients)
                    c.Close();
                clients.Clear();
            }
            try
            {
                var file = Path.Combine(projectPath, InstanceFile);
                // Leave a newer editor session's file alone.
                if (File.Exists(file) && File.ReadAllText(file).Contains(token))
                    File.Delete(file);
            }
            catch (Exception) { }
        }

        static void AcceptLoop()
        {
            while (true)
            {
                var l = listener;
                if (l == null)
                    return;
                TcpClient tcp;
                try
                {
                    tcp = l.AcceptTcpClient();
                }
                catch (Exception)
                {
                    return;
                }
                var client = new Client(tcp);
                lock (clientsLock)
                    clients.Add(client);
                new Thread(() => ReadLoop(client)) { IsBackground = true, Name = "Forge bridge client" }.Start();
            }
        }

        static void ReadLoop(Client client)
        {
            try
            {
                string line;
                while ((line = client.ReadLine()) != null)
                {
                    if (line.Length == 0)
                        continue;
                    Message m;
                    try { m = JsonUtility.FromJson<Message>(line); }
                    catch (Exception) { continue; }
                    if (m == null || string.IsNullOrEmpty(m.type))
                        continue;
                    if (!client.Authenticated)
                    {
                        if (m.type != "hello" || m.token != token)
                        {
                            client.Send(JsonUtility.ToJson(new ErrorMessage { type = "error", message = "wrong token" }));
                            break;
                        }
                        client.Authenticated = true;
                        client.Send(JsonUtility.ToJson(new WelcomeMessage
                        {
                            type = "welcome",
                            protocol = Protocol,
                            unityVersion = Application.unityVersion,
                            projectPath = projectPath.Replace('\\', '/'),
                            pid = pid,
                            playMode = playMode,
                            compiling = compiling,
                        }));
                        continue;
                    }
                    var command = m.type;
                    var id = m.id;
                    mainThread.Enqueue(() => Execute(client, command, id));
                }
            }
            catch (Exception) { }
            client.Close();
            lock (clientsLock)
                clients.Remove(client);
        }

        static void Execute(Client client, string command, long id)
        {
            var ok = true;
            string message = null;
            try
            {
                switch (command)
                {
                    case "play":
                        EditorApplication.isPlaying = true;
                        break;
                    case "pause":
                        EditorApplication.isPaused = true;
                        break;
                    case "resume":
                        EditorApplication.isPaused = false;
                        break;
                    case "stop":
                        EditorApplication.isPlaying = false;
                        break;
                    case "step":
                        EditorApplication.Step();
                        break;
                    case "refresh":
                        AssetDatabase.Refresh();
                        break;
                    case "syncSolution":
                        ok = ForgeProjectFiles.Sync(out message);
                        break;
                    default:
                        ok = false;
                        message = "unknown command " + command;
                        break;
                }
            }
            catch (Exception e)
            {
                ok = false;
                message = e.Message;
            }
            client.Send(JsonUtility.ToJson(new AckMessage { type = "ack", id = id, ok = ok, message = message ?? "" }));
        }

        static void Pump()
        {
            while (mainThread.TryDequeue(out var action))
                action();
        }

        static string CurrentPlayMode()
        {
            if (!EditorApplication.isPlaying)
                return "edit";
            return EditorApplication.isPaused ? "paused" : "playing";
        }

        static void OnStateChanged()
        {
            playMode = CurrentPlayMode();
            Broadcast(StateMessage());
        }

        static string StateMessage()
        {
            return JsonUtility.ToJson(new StateMessageData { type = "state", playMode = playMode, compiling = compiling });
        }

        static void OnLog(string condition, string stackTrace, LogType type)
        {
            if (!IsConnected)
                return;
            string level;
            switch (type)
            {
                case LogType.Warning: level = "warning"; break;
                case LogType.Error: level = "error"; break;
                case LogType.Exception: level = "exception"; break;
                case LogType.Assert: level = "assert"; break;
                default: level = "log"; break;
            }
            Broadcast(JsonUtility.ToJson(new LogMessage
            {
                type = "log",
                level = level,
                message = condition ?? "",
                stackTrace = stackTrace ?? "",
                time = DateTime.Now.ToString("HH:mm:ss"),
            }));
        }

        static void Broadcast(string json)
        {
            Client[] targets;
            lock (clientsLock)
                targets = clients.FindAll(c => c.Authenticated).ToArray();
            foreach (var c in targets)
                c.Send(json);
        }

        static string NewToken()
        {
            var bytes = new byte[24];
            using (var rng = RandomNumberGenerator.Create())
                rng.GetBytes(bytes);
            var sb = new StringBuilder();
            foreach (var b in bytes)
                sb.Append(b.ToString("x2"));
            return sb.ToString();
        }

        sealed class Client
        {
            readonly TcpClient tcp;
            readonly Stream stream;
            readonly object writeLock = new object();
            readonly List<byte> pending = new List<byte>();
            readonly byte[] buffer = new byte[8192];
            int bufferLength;
            int bufferPos;
            public volatile bool Authenticated;

            public Client(TcpClient tcp)
            {
                this.tcp = tcp;
                tcp.NoDelay = true;
                stream = tcp.GetStream();
            }

            /// <summary>One line without its end; null at the end of the stream or for an oversized message.</summary>
            public string ReadLine()
            {
                pending.Clear();
                while (true)
                {
                    if (bufferPos == bufferLength)
                    {
                        bufferLength = stream.Read(buffer, 0, buffer.Length);
                        bufferPos = 0;
                        if (bufferLength <= 0)
                            return null;
                    }
                    var b = buffer[bufferPos++];
                    if (b == (byte)'\n')
                        return Encoding.UTF8.GetString(pending.ToArray()).TrimEnd('\r');
                    pending.Add(b);
                    if (pending.Count > MaxMessage)
                        return null;
                }
            }

            public void Send(string json)
            {
                var bytes = Encoding.UTF8.GetBytes(json + "\n");
                lock (writeLock)
                {
                    try
                    {
                        stream.Write(bytes, 0, bytes.Length);
                        stream.Flush();
                    }
                    catch (Exception)
                    {
                        Close();
                    }
                }
            }

            public void Close()
            {
                Authenticated = false;
                try { tcp.Close(); } catch (Exception) { }
            }
        }

        [Serializable] class Message { public string type; public long id; public string token; public string client; public int protocol; }
        [Serializable] class InstanceInfo { public int port; public int pid; public string token; public int protocol; public string unityVersion; public string projectPath; }
        [Serializable] class WelcomeMessage { public string type; public int protocol; public string unityVersion; public string projectPath; public int pid; public string playMode; public bool compiling; }
        [Serializable] class StateMessageData { public string type; public string playMode; public bool compiling; }
        [Serializable] class LogMessage { public string type; public string level; public string message; public string stackTrace; public string time; }
        [Serializable] class OpenFileMessage { public string type; public string path; public int line; public int column; }
        [Serializable] class AckMessage { public string type; public long id; public bool ok; public string message; }
        [Serializable] class ErrorMessage { public string type; public string message; }
    }
}
