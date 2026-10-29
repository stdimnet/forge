"""Forge's Python Console: a REPL driven by JSON requests on stdin. Output of the user's
code goes to stdout / stderr as usual; replies to Forge are JSON lines that start with
the record separator (\\x1e) on stdout.

Requests (one JSON object per line):
    {"op": "exec", "code": "..."}             run input (incomplete input asks for more)
    {"op": "runfile", "path": "..."}          run a file in the console's namespace
    {"op": "vars"}                            the variables of the namespace
    {"op": "children", "expr": "orders[0]"}   the members of a value
    {"op": "input", "text": "..."}            the answer to the program's input()

Replies: {"type": "ready", "banner", "prompt", "more", "ipython"},
{"type": "more", "prompt"}, {"type": "done", "prompt"}, {"type": "vars", "vars"},
{"type": "children", "expr", "vars"}, {"type": "input", "prompt"}.

IPython runs the input when it is installed (magics, rich display); the standard
library's interactive interpreter otherwise. Python 3.7+.
"""
import ast
import builtins
import code
import json
import os
import platform
import runpy
import sys
import traceback

RS = "\x1e"
OUT = sys.stdout
IN = sys.stdin
HIDDEN = {"__builtins__", "__name__", "__doc__", "__package__", "__loader__", "__spec__",
          "__file__", "__cached__", "In", "Out", "exit", "quit", "get_ipython", "_oh", "_ih",
          "_dh", "_", "__", "___"}
SIMPLE = (int, float, complex, bool, str, bytes, type(None))


def send(obj):
    OUT.flush()
    sys.stderr.flush()
    OUT.write(RS + json.dumps(obj) + "\n")
    OUT.flush()


def short_repr(value, limit=200):
    try:
        r = repr(value)
    except Exception as e:  # noqa: BLE001 - a broken __repr__ must not stop the console
        r = "<repr failed: %s>" % e
    return r if len(r) <= limit else r[: limit - 1] + "…"


def size_of(value):
    try:
        if isinstance(value, (str, bytes)):
            return None
        return len(value)
    except Exception:  # noqa: BLE001
        return None


def has_children(value):
    if isinstance(value, SIMPLE):
        return False
    if isinstance(value, (list, tuple, set, frozenset, dict)):
        return len(value) > 0
    return bool(getattr(value, "__dict__", None))


def describe(name, value, expr):
    kind = type(value)
    return {
        "name": str(name),
        "type": kind.__name__ if kind.__module__ == "builtins" else "%s.%s" % (kind.__module__, kind.__name__),
        "value": short_repr(value),
        "size": size_of(value),
        "expr": expr,
        "children": has_children(value),
    }


def variables(ns):
    out = []
    for name in sorted(ns):
        if name in HIDDEN or name.startswith("_"):
            continue
        value = ns[name]
        if type(value).__name__ == "module" or name in ("forge_console",):
            continue
        out.append(describe(name, value, name))
    return out


def children(ns, expr):
    value = eval(expr, ns)  # noqa: S307 - the user's own console
    items = []
    if isinstance(value, dict):
        for i, (k, v) in enumerate(value.items()):
            if i >= 300:
                break
            items.append(describe(short_repr(k, 60), v, "%s[%r]" % (expr, k)))
    elif isinstance(value, (list, tuple)):
        for i, v in enumerate(value[:300]):
            items.append(describe(i, v, "%s[%d]" % (expr, i)))
    elif isinstance(value, (set, frozenset)):
        for i, v in enumerate(list(value)[:300]):
            items.append(describe(i, v, "list(%s)[%d]" % (expr, i)))
    else:
        for k, v in sorted(getattr(value, "__dict__", {}).items()):
            if not k.startswith("__"):
                items.append(describe(k, v, "%s.%s" % (expr, k)))
    return items


def read_request():
    line = IN.readline()
    if not line:
        return None
    try:
        return json.loads(line)
    except ValueError:
        return {"op": "exec", "code": line.rstrip("\n")}


def console_input(prompt=""):
    send({"type": "input", "prompt": str(prompt)})
    while True:
        req = read_request()
        if req is None:
            raise EOFError
        if req.get("op") == "input":
            return req.get("text", "")


class Plain:
    """The standard library's interactive interpreter."""

    ipython = False
    prompt, more = ">>> ", "... "

    def __init__(self):
        self.ns = {"__name__": "__main__", "__builtins__": builtins}
        self.interp = code.InteractiveInterpreter(self.ns)

    def complete(self, source):
        try:
            return code.compile_command(source, "<console>", "single") is not None
        except (SyntaxError, OverflowError, ValueError):
            return True

    def run(self, source):
        try:
            tree = ast.parse(source, "<console>", "exec")
        except SyntaxError:
            self.interp.showsyntaxerror("<console>")
            return
        body = tree.body
        try:
            if body and isinstance(body[-1], ast.Expr):
                head = ast.Module(body=body[:-1], type_ignores=[])
                exec(compile(head, "<console>", "exec"), self.ns)  # noqa: S102
                last = ast.Interactive(body=[body[-1]])
                exec(compile(last, "<console>", "single"), self.ns)  # noqa: S102
            else:
                exec(compile(tree, "<console>", "exec"), self.ns)  # noqa: S102
        except SystemExit:
            raise
        except BaseException:  # noqa: BLE001 - shown like the REPL does
            self.interp.showtraceback()

    def runfile(self, path):
        self.ns.update(runpy.run_path(path, init_globals=self.ns, run_name="__main__"))


class IPython:
    """IPython's shell without a terminal."""

    ipython = True

    def __init__(self, shell):
        self.shell = shell
        self.ns = shell.user_ns

    @property
    def prompt(self):
        return "In [%d]: " % self.shell.execution_count

    @property
    def more(self):
        return "   ...: "

    def complete(self, source):
        status, _ = self.shell.input_transformer_manager.check_complete(source + "\n")
        return status != "incomplete"

    def run(self, source):
        self.shell.run_cell(source, store_history=True)

    def runfile(self, path):
        self.shell.safe_execfile(path, self.ns, raise_exceptions=False, shell_futures=False)


def make_console(use_ipython):
    if use_ipython:
        try:
            from IPython.core.interactiveshell import InteractiveShell

            shell = InteractiveShell.instance(colors="NoColor")
            return IPython(shell)
        except Exception:  # noqa: BLE001 - IPython missing or broken: the plain REPL
            pass
    return Plain()


def main():
    use_ipython = "--no-ipython" not in sys.argv
    builtins.input = console_input
    sys.path.insert(0, os.getcwd())
    if os.path.isdir("src"):
        sys.path.insert(1, os.path.abspath("src"))
    console = make_console(use_ipython)
    banner = "Python %s (%s) on %s" % (platform.python_version(), platform.python_implementation(), sys.platform)
    if console.ipython:
        import IPython as ip

        banner += "\nIPython %s -- An enhanced Interactive Python." % ip.__version__
    send({"type": "ready", "banner": banner, "prompt": console.prompt, "more": console.more,
          "ipython": console.ipython})
    send({"type": "vars", "vars": variables(console.ns)})
    while True:
        req = read_request()
        if req is None:
            break
        op = req.get("op")
        try:
            if op == "exec":
                source = req.get("code", "")
                if not req.get("block") and not console.complete(source):
                    send({"type": "more", "prompt": console.more})
                    continue
                console.run(source)
            elif op == "runfile":
                console.runfile(req.get("path", ""))
            elif op == "children":
                send({"type": "children", "expr": req.get("expr"),
                      "vars": children(console.ns, req.get("expr", ""))})
                continue
            elif op != "vars":
                continue
        except SystemExit:
            break
        except BaseException:  # noqa: BLE001
            traceback.print_exc()
        send({"type": "done", "prompt": console.prompt})
        send({"type": "vars", "vars": variables(console.ns)})


main()
