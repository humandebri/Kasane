"""Check the real unchecked_add precondition before/after panic normalization."""
import json
import sys


def nodes(value):
    yield value
    if isinstance(value, dict):
        for child in value.values():
            yield from nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from nodes(child)


def precondition(path):
    crate = json.load(open(path))["translated"]
    assert not json.load(open(path))["has_errors"]
    return next(fun for fun in crate["fun_decls"] if fun and
                any(part.get("Ident", [""])[0] == "precondition_check"
                    for part in fun["item_meta"]["name"]))


before, after = (precondition(path) for path in sys.argv[1:])
assert not any(isinstance(node, dict) and "Panic" in node
               for node in nodes(before["body"]))
assert any(isinstance(node, dict) and "Panic" in node and
           [part["Ident"][0] for part in node["Panic"]["name"]] ==
           ["core", "panicking", "panic_nounwind_fmt"]
           for node in nodes(after["body"]))
print("unchecked_add precondition: nounwind panic normalization regression passed")
