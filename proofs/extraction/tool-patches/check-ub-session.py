"""Check literal UB settings against real stock/patched extraction output."""
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


def body(path):
    crate = json.load(open(path))
    assert not crate["has_errors"]
    return next(fun["body"] for fun in crate["translated"]["fun_decls"]
                if fun and fun["item_meta"]["is_local"])


before, enabled, disabled = (body(path) for path in sys.argv[1:])
assert any(node == {"NullaryOp": "UbChecks"} for node in nodes(before))
for program, expected in [(enabled, True), (disabled, False)]:
    assert not any(isinstance(node, dict) and "NullaryOp" in node
                   for node in nodes(program))
    assert [node["Bool"] for node in nodes(program)
            if isinstance(node, dict) and "Bool" in node] == [expected]
print("UB session regression passed for enabled and disabled settings")
