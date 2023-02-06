
try:
    import ghidra_bridge
    has_bridge=True
except ImportError:
    has_bridge=False

from contextlib import contextmanager

if has_bridge:
    import ghidra_bridge
    b = ghidra_bridge.GhidraBridge(namespace=globals(), hook_import=True)
    @contextmanager
    def transaction():
        start()
        try:
            yield
        except Exception as e:
            end(False)
            raise e
        end(True)
else:
    @contextmanager
    def transaction():
        yield

import ghidra.program.model.symbol.SymbolType as SymbolType
import ghidra.program.model.symbol.SourceType as SourceType
from ghidra.app.cmd.label import CreateNamespacesCmd
from ghidra.program.model.data.DataUtilities import createData
from ghidra.program.model.data.DataUtilities import ClearDataMode
from ghidra.program.model.listing.CodeUnit import PLATE_COMMENT
def make_namespace(parts):
    ns_cmd = CreateNamespacesCmd("::".join(parts), SourceType.USER_DEFINED)
    ns_cmd.applyTo(currentProgram)
    return ns_cmd.getNamespace()


callback_refs = [ref.fromAddress for ref in getReferencesTo(toAddr(0x590C70)).tolist()]
engine_var_refs = [
    ref.fromAddress for ref in getReferencesTo(toAddr(0x5319D0)).tolist()
]

dtm = currentProgram.getDataTypeManager()
engine_var_dt = dtm.getDataType("/EngineVar")
callback_dt = dtm.getDataType("/CCallback")

def create_data(addr,dtype):
    return createData(currentProgram,addr,dtype,0,False,ClearDataMode.CLEAR_ALL_CONFLICT_DATA)

def create_str(addr):
    str_len = (findBytes(addr, b"\0").offset - addr.offset) + 1
    clearListing(addr, addr.add(str_len))
    return createAsciiString(addr)


def make_namespace(parts):
    ns_cmd = CreateNamespacesCmd("::".join(parts), SourceType.USER_DEFINED)
    ns_cmd.applyTo(currentProgram)
    return ns_cmd.getNamespace()


def get_call_obj(addr):
    func = getFunctionContaining(addr)
    if func is None:
        disassemble(addr)
        func = createFunction(addr,None)
    call_obj = {"this": None, "stack": []}
    for inst in currentProgram.listing.getInstructions(func.body, True):
        affected_objs = [r.toString() for r in inst.resultObjects.tolist()]
        inst_name = inst.getMnemonicString()
        if inst_name == "PUSH":
            val=inst.getScalar(0)
            if val is not None:
                call_obj["stack"].insert(0, toAddr(val.getValue()).toString())
        elif inst_name == "MOV" and "ECX" in affected_objs:
            this = inst.getScalar(1)
            if this is not None:
                call_obj["this"] = toAddr(this.getValue()).toString()
        elif inst_name == "CALL":
            break
    return func, call_obj


with transaction():
    for ref in callback_refs:
        register_callback, call_obj = get_call_obj(ref)
        name, addr = call_obj["stack"]
        this = toAddr(call_obj["this"])
        addr = toAddr(addr)
        name = create_str(toAddr(name)).getValue()
        callback_ns = make_namespace(["Callbacks"])
        ns = make_namespace(["Callbacks", name])
        clearListing(addr)
        disassemble(addr)
        func = createFunction(addr,None)
        print(name,func)
        createLabel(addr, name, callback_ns, True, SourceType.USER_DEFINED)
        createLabel(
            register_callback.getEntryPoint(),
            "register",
            ns,
            True,
            SourceType.USER_DEFINED,
        )
        createLabel(this, name, None, True, SourceType.USER_DEFINED)
        create_data(this,callback_dt)

    for ref in engine_var_refs:
        register_engine_var, call_obj = get_call_obj(ref)
        engine_var = call_obj['this']
        try:
            name,flags,desc = call_obj['stack'][:3]
        except ValueError:
            continue
        name=create_str(toAddr(name)).getValue()
        desc=create_str(toAddr(desc)).getValue()
        print(name,ref)
        ev_ns = make_namespace(["EngineVars"])
        ns = make_namespace(["EngineVars", name])
        clearListing(toAddr(engine_var))
        create_data(toAddr(engine_var),engine_var_dt).setComment(PLATE_COMMENT,desc)
        createLabel(toAddr(engine_var), name, ev_ns, True, SourceType.USER_DEFINED)
        clearListing(register_engine_var.getEntryPoint())
        createLabel(register_engine_var.getEntryPoint(), "register", ns, True, SourceType.USER_DEFINED)

# listing = currentProgram.getListing()
# codeUnit = listing.getCodeUnitAt(minAddress)
# codeUnit.setComment(codeUnit.PLATE_COMMENT, "AddCommentToProgramScript - This is an added comment!")


# dtm = currentProgram.getDataTypeManager()
# dt_engine_var = dtm.getDataType("/EngineVar")
# dt_engine_ptr = dtm.getPointer(dt_engine_var)
