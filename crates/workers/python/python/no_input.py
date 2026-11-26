import builtins

def _no_input(prompt=""):
    raise RuntimeError("input() needs the page to be served with COOP and COEP headers")

builtins.input = _no_input
