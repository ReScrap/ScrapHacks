from ghidra.app.decompiler import DecompileOptions
from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor

TARGET_FUNC = "add_callback"

def xref_params(target_func):
    target_addr = 0
    callers = []
    funcs = getGlobalFunctions(target_func)
    for func in funcs:
        if func.getName() == target_func:
            target_addr = func.getEntryPoint()
            references = getReferencesTo(target_addr)
            for xref in references:
                call_addr = xref.getFromAddress()
                caller = getFunctionContaining(call_addr)
                callers.append(caller)
            break
    callers = list(set(callers))
    options = DecompileOptions()
    monitor = ConsoleTaskMonitor()
    ifc = DecompInterface()
    ifc.setOptions(options)
    ifc.openProgram(currentProgram)
    with open("callbacks.md", "w") as file:
        res = "|Callback setup address|Callback name|Callback funcion|Callback address|"
        print(res)
        file.write(res + "\n")
        res = "|-----|----|----|--------|"
        print(res)
        file.write(res + "\n")
        for caller in callers:
            callback_setup_addr = caller.getEntryPoint()
            res = ifc.decompileFunction(caller, 60, monitor)
            code = str(res.getDecompiledFunction().getC())
            code = code.split(target_func)[1]
            code = code.split(';')[0]
            code = code.strip()
            code = code.split(',')
            callback_name = code[1].strip()
            callback_func = code[2].strip()[:-1].strip().replace('_', '.')
            res = ifc.decompileFunction(caller, 60, monitor)
            hf = res.getHighFunction()
            opiter = hf.getPcodeOps()
            callback_addr = "not found"
            while opiter.hasNext():
                op = opiter.next()
                mnemonic = op.getMnemonic()
                if mnemonic == "CALL":
                    core_func = op.getInput(3)
                    callback_addr = toAddr(core_func.getDef().getInput(1).getOffset())
            res = "|`{}`|{}|`{}`|`{}`|".format(callback_setup_addr, callback_name, callback_func, callback_addr)
            print(res)
            file.write(res + "\n")


xref_params(TARGET_FUNC)