# Forge's helper inside a Jupyter kernel: the Variables view and View as DataFrame. Sent as
# silent code before `user_expressions` that call it; `as_json` values print as plain JSON
# (their repr), so the reply needs no unquoting. Defined once per kernel process.
if "__forge_jupyter" not in globals():
    def __forge_jupyter_setup(ns):
        import json

        hidden = {"In", "Out", "exit", "quit", "get_ipython", "open"}
        simple = (int, float, complex, bool, str, bytes, type(None))

        class AsJson(object):
            __slots__ = ("text",)

            def __init__(self, text):
                self.text = text

            def __repr__(self):
                return self.text

            def _repr_pretty_(self, p, cycle):
                p.text(self.text)

        def short_repr(value, limit=200):
            try:
                r = repr(value)
            except Exception as e:  # noqa: BLE001 - a broken __repr__ must not break the view
                r = "<repr failed: %s>" % e
            return r if len(r) <= limit else r[: limit - 1] + "…"

        def type_name(value):
            kind = type(value)
            if kind.__module__ == "builtins":
                return kind.__name__
            return "%s.%s" % (kind.__module__, kind.__name__)

        def is_frame(value):
            mod = type(value).__module__ or ""
            if mod.startswith("pandas") and type(value).__name__ in ("DataFrame", "Series"):
                return True
            return mod.startswith("numpy") and type(value).__name__ == "ndarray" and getattr(value, "ndim", 0) in (1, 2)

        def size_of(value):
            shape = getattr(value, "shape", None)
            if isinstance(shape, tuple) and is_frame(value):
                return None
            try:
                if isinstance(value, (str, bytes)):
                    return None
                return len(value)
            except Exception:  # noqa: BLE001
                return None

        def has_children(value):
            if isinstance(value, simple) or is_frame(value):
                return False
            if isinstance(value, (list, tuple, set, frozenset, dict)):
                return len(value) > 0
            return bool(getattr(value, "__dict__", None))

        def describe(name, value, expr):
            out = {
                "name": str(name),
                "type": type_name(value),
                "value": short_repr(value),
                "size": size_of(value),
                "expr": expr,
                "children": has_children(value),
            }
            if is_frame(value):
                out["frame"] = True
                shape = getattr(value, "shape", None)
                if shape is not None:
                    out["value"] = "%s %s" % (type(value).__name__, "×".join(str(s) for s in shape))
            return out

        class Api(object):
            def as_json(self, value):
                return AsJson(json.dumps(value, default=str))

            def variables(self):
                out = []
                for name in sorted(ns):
                    if name in hidden or name.startswith("_"):
                        continue
                    value = ns[name]
                    if type(value).__name__ == "module" or callable(value) and not is_frame(value) and type(value).__name__ in ("function", "builtin_function_or_method", "type"):
                        continue
                    out.append(describe(name, value, name))
                return out

            def children(self, expr):
                value = eval(expr, ns)  # noqa: S307 - the user's own kernel
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

            def frame(self, expr, limit=10000):
                import pandas as pd

                value = eval(expr, ns)  # noqa: S307 - the user's own kernel
                if not isinstance(value, pd.DataFrame):
                    value = pd.DataFrame(value)
                head = value.head(limit)

                def cell(x):
                    if x is None:
                        return ""
                    return str(x)

                index_names = [cell(n) for n in head.index.names]
                columns = [" / ".join(str(p) for p in c) if isinstance(c, tuple) else str(c) for c in head.columns]
                rows = []
                for idx, row in zip(head.index, head.itertuples(index=False, name=None)):
                    idx = idx if isinstance(idx, tuple) else (idx,)
                    rows.append([cell(x) for x in idx] + [cell(x) for x in row])
                return {
                    "columns": index_names + columns,
                    "dtypes": [""] * len(index_names) + [str(t) for t in head.dtypes],
                    "indexColumns": len(index_names),
                    "rows": rows,
                    "totalRows": len(value),
                    "totalColumns": len(value.columns),
                }

        return Api()

    __forge_jupyter = __forge_jupyter_setup(globals())
    del __forge_jupyter_setup
