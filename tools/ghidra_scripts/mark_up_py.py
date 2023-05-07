import time
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

listing = currentProgram.getListing()
dtm = currentProgram.getDataTypeManager()
py_mod = dtm.getDataType("/PyModuleDef")
py_meth = dtm.getDataType("/PyMethodDef")

NULL=toAddr(0)

def make_namespace(parts):
    ns_cmd = CreateNamespacesCmd("::".join(parts), SourceType.USER_DEFINED)
    ns_cmd.applyTo(currentProgram)
    return ns_cmd.getNamespace()

def create_data(addr,dtype):
    return createData(currentProgram,addr,dtype,0,False,ClearDataMode.CLEAR_ALL_CONFLICT_DATA)

def create_str(addr):
    if addr.equals(NULL):
        return None
    str_len = (findBytes(addr, b"\0").offset - addr.offset) + 1
    clearListing(addr, addr.add(str_len))
    return createAsciiString(addr)

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
    func=func.symbol.address
    return func, call_obj

def data_to_dict(data):
    ret={}
    for idx in range(data.dataType.getNumComponents()):
        name=data.dataType.getComponent(idx).getFieldName()
        value=data.getComponent(idx).getValue()
        ret[name]=value
    return ret

def try_create_str(addr):
    ret=create_str(addr)
    if ret:
        return ret.getValue()

with transaction():
    PyInitModule=getSymbolAt(toAddr("006f31c0"))
    for ref in getReferencesTo(PyInitModule.address).tolist():
        func,args=get_call_obj(ref.fromAddress)
        print(func,args)
        module_name=create_str(toAddr(args['stack'][0])).getValue()
        methods=toAddr(args['stack'][1])
        module_doc=create_str(toAddr(args['stack'][2]))
        if module_doc:
            module_doc=module_doc.getValue()
        print(methods,module_name,module_doc)
        mod_ns = make_namespace(["Python", module_name])
        createLabel(func, "__init__", mod_ns, True, SourceType.USER_DEFINED)
        if module_doc:
            listing.getCodeUnitAt(func).setComment(PLATE_COMMENT,module_doc)
        while True:
            mod_data=data_to_dict(create_data(methods,py_meth))
            if mod_data['name'] is None:
                clearListing(methods, methods.add(16))
                break
            mod_data['name']=try_create_str(mod_data['name'])
            try:
                mod_data['doc']=try_create_str(mod_data['doc'])
            except:
                mod_data['doc']=None
            print(mod_data)
            createLabel(mod_data['ml_method'], mod_data['name'], mod_ns, True, SourceType.USER_DEFINED)
            if mod_data['doc']:
                listing.getCodeUnitAt(mod_data['ml_method']).setComment(PLATE_COMMENT,module_doc)
            methods=methods.add(16)
            try:
                if getBytes(methods,4).tolist()==[0,0,0,0]:
                    break
            except:
                break