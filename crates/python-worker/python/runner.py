import sys, traceback
from pyodide.code import eval_code_async

async def run(code, path):
    try:
        await eval_code_async(code, {"__name__": "__main__", "__file__": path}, filename=path)
    except SystemExit as e:
        if e.code not in (None, 0):
            print(e.code, file=sys.stderr)
    except BaseException as e:
        tb = e.__traceback__
        while tb and tb.tb_frame.f_code.co_filename != path:
            tb = tb.tb_next
        traceback.print_exception(type(e), e, tb)
    finally:
        sys.stdout.flush()
        sys.stderr.flush()

run
