// The `forge` API inside a plugin's sandboxed iframe (`sandbox="allow-scripts"`, opaque
// origin, CSP `default-src 'none'`). It has no access to the IDE's DOM, IPC or network: every
// call is a message to the host (`features/plugins/host.ts`), which checks the plugin's
// permissions and forwards backend calls to Rust. `__FORGE_PLUGIN__` is replaced with the
// plugin's id, name and version. Keep this file plain ES2020 without imports.
(function () {
  "use strict";
  var PLUGIN = __FORGE_PLUGIN__;
  var seq = 0;
  var pending = new Map();
  var commands = new Map();
  var listeners = new Map();

  function post(message) {
    message.forgePlugin = 1;
    parent.postMessage(message, "*");
  }

  function call(method) {
    var args = Array.prototype.slice.call(arguments, 1);
    return new Promise(function (resolve, reject) {
      var id = ++seq;
      pending.set(id, { resolve: resolve, reject: reject });
      post({ kind: "call", id: id, method: method, args: args });
    });
  }

  function on(name, callback) {
    if (!listeners.has(name)) listeners.set(name, new Set());
    listeners.get(name).add(callback);
    return {
      dispose: function () {
        listeners.get(name).delete(callback);
      },
    };
  }

  window.addEventListener("message", function (event) {
    if (event.source !== parent) return;
    var m = event.data;
    if (!m || m.forgeHost !== 1) return;
    if (m.kind === "result") {
      var p = pending.get(m.id);
      if (!p) return;
      pending.delete(m.id);
      if (m.error !== undefined) p.reject(new Error(m.error));
      else p.resolve(m.value);
    } else if (m.kind === "command") {
      var handler = commands.get(m.command);
      Promise.resolve()
        .then(function () {
          if (!handler) throw new Error("command " + m.command + " is not registered");
          return handler.apply(null, m.args || []);
        })
        .then(
          function () {
            post({ kind: "commandDone", id: m.id });
          },
          function (e) {
            post({ kind: "commandDone", id: m.id, error: String((e && e.message) || e) });
          },
        );
    } else if (m.kind === "event") {
      var set = listeners.get(m.name);
      if (set)
        set.forEach(function (cb) {
          try {
            cb(m.payload);
          } catch (e) {
            console.error(e);
          }
        });
    }
  });

  var forge = {
    apiVersion: 1,
    plugin: PLUGIN,
    commands: {
      register: function (id, handler) {
        commands.set(id, handler);
        return {
          dispose: function () {
            commands.delete(id);
          },
        };
      },
      execute: function (id) {
        return call("commands.execute", id);
      },
    },
    statusBar: {
      update: function (id, item) {
        return call("statusBar.update", id, item);
      },
    },
    toolWindows: {
      setHtml: function (id, html) {
        return call("toolWindows.setHtml", id, html);
      },
      postMessage: function (id, data) {
        return call("toolWindows.postMessage", id, data);
      },
      onMessage: function (id, callback) {
        return on("toolWindow:" + id, callback);
      },
    },
    editor: {
      getActive: function () {
        return call("editor.getActive");
      },
      replaceSelection: function (text) {
        return call("editor.replaceSelection", text);
      },
      onDidChange: function (callback) {
        return on("editor.changed", callback);
      },
    },
    window: {
      showMessage: function (text, type) {
        return call("window.showMessage", String(text), type || "info");
      },
    },
    settings: {
      get: function (key) {
        return call("settings.get", key);
      },
      set: function (key, value) {
        return call("settings.set", key, value);
      },
      onDidChange: function (callback) {
        return on("settings.changed", callback);
      },
    },
    workspace: {
      root: function () {
        return call("workspace.root");
      },
    },
    fs: {
      readFile: function (path) {
        return call("fs.readFile", path);
      },
      writeFile: function (path, contents) {
        return call("fs.writeFile", path, contents);
      },
    },
    http: {
      fetch: function (url, init) {
        return call("http.fetch", Object.assign({ url: url }, init || {}));
      },
    },
    process: {
      run: function (program, args) {
        return call("process.run", program, args || []);
      },
    },
  };
  Object.freeze(forge);
  globalThis.forge = forge;

  // Called after the plugin's bundle ran: it defined `__forgePlugin` (an esbuild IIFE with
  // `globalName`) whose `activate(context)` sets everything up.
  globalThis.__forgeActivate = function () {
    var plugin = globalThis.__forgePlugin || {};
    var context = { subscriptions: [], plugin: PLUGIN };
    Promise.resolve()
      .then(function () {
        return typeof plugin.activate === "function" ? plugin.activate(context) : undefined;
      })
      .then(
        function () {
          post({ kind: "activated" });
        },
        function (e) {
          post({ kind: "activated", error: String((e && e.stack) || e) });
        },
      );
  };
})();
