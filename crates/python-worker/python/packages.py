import ast, importlib, importlib.util, json, os, sys

_synced = set()
_SKIP_DIRS = {"__pycache__", "tests", "test"}
_MAX_FILE = 1_000_000
_MAX_PACKAGE = 40_000_000

def wanted(code):
    try:
        tree = ast.parse(code)
    except SyntaxError:
        return "[]"
    names = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            names.update(a.name.split(".")[0] for a in node.names)
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            names.add(node.module.split(".")[0])
    names -= set(sys.stdlib_module_names)
    names -= _synced
    return json.dumps(sorted(names))

def collect(names):
    packages = []
    for name in json.loads(names):
        _synced.add(name)
        try:
            importlib.invalidate_caches()
            spec = importlib.util.find_spec(name)
        except Exception:
            continue
        if spec is None or not spec.origin or "site-packages" not in spec.origin:
            continue
        is_package = spec.submodule_search_locations is not None
        root = os.path.dirname(spec.origin) if is_package else spec.origin
        base = os.path.dirname(root)
        files, size = [], 0
        if not is_package:
            sources = [(os.path.basename(spec.origin), spec.origin)]
        else:
            sources = []
            for folder, dirs, entries in os.walk(root):
                dirs[:] = [d for d in dirs if d not in _SKIP_DIRS]
                present = set(entries)
                for entry in entries:
                    stem, ext = os.path.splitext(entry)
                    if ext == ".pyi" or (ext == ".py" and stem + ".pyi" not in present):
                        full = os.path.join(folder, entry)
                        sources.append((os.path.relpath(full, base), full))
        for rel, full in sources:
            try:
                if os.path.getsize(full) > _MAX_FILE or size > _MAX_PACKAGE:
                    continue
                with open(full, encoding="utf-8") as f:
                    text = f.read()
            except (OSError, UnicodeDecodeError):
                continue
            size += len(text)
            files.append([rel.replace(os.sep, "/"), text])
        packages.append({"name": name, "files": files})
    return json.dumps(packages)

(wanted, collect)
