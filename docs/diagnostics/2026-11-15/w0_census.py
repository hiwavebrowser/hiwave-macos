# W0 (STR-3 WebAssembly): census of a .wasm binary (sections, imports, exports, memory/table limits, instruction families).
# Usage: python3 w0_census.py a.wasm [b.wasm ...]. Pure Python, no dependencies. Written 2026-11-15 by Talos.
import sys, struct, collections, json

def uleb(b, i):
    r = s = 0
    while True:
        x = b[i]; i += 1; r |= (x & 0x7f) << s; s += 7
        if not x & 0x80: return r, i
def sleb(b, i):
    r = s = 0
    while True:
        x = b[i]; i += 1; r |= (x & 0x7f) << s; s += 7
        if not x & 0x80:
            if x & 0x40: r -= 1 << s
            return r, i
def name(b, i):
    n, i = uleb(b, i); return b[i:i+n].decode('utf8', 'replace'), i + n
VT = {0x7f:'i32',0x7e:'i64',0x7d:'f32',0x7c:'f64',0x7b:'v128',0x70:'funcref',0x6f:'externref'}
def limits(b, i):
    f, i = uleb(b, i); mn, i = uleb(b, i); mx = None
    if f & 1: mx, i = uleb(b, i)
    return dict(min=mn, max=mx, shared=bool(f & 2), mem64=bool(f & 4)), i

# immediates for MVP single-byte opcodes
def skip_blocktype(b, i):
    if b[i] == 0x40 or b[i] in VT: return i + 1
    _, i = sleb(b, i); return i

def census(path):
    b = open(path, 'rb').read(); assert b[:4] == b'\0asm'
    out = dict(file=path.split('/')[-1], bytes=len(b), version=struct.unpack('<I', b[4:8])[0])
    i = 8; types = []; imports = []; exports = []; funcs = 0; mem = []; tab = []; glob = []; start = None; data = []; elem = 0; ncode = 0; custom = []
    code_range = None
    while i < len(b):
        sid = b[i]; i += 1; sz, i = uleb(b, i); end = i + sz; j = i
        if sid == 0:
            n, j = name(b, j); custom.append((n, sz))
        elif sid == 1:
            n, j = uleb(b, j)
            for _ in range(n):
                assert b[j] == 0x60; j += 1
                np, j = uleb(b, j); ps = [VT[b[j+k]] for k in range(np)]; j += np
                nr, j = uleb(b, j); rs = [VT[b[j+k]] for k in range(nr)]; j += nr
                types.append((ps, rs))
        elif sid == 2:
            n, j = uleb(b, j)
            for _ in range(n):
                m, j = name(b, j); f, j = name(b, j); k = b[j]; j += 1
                if k == 0: t, j = uleb(b, j); imports.append((m, f, 'func', types[t]))
                elif k == 1: ty = b[j]; j += 1; l, j = limits(b, j); imports.append((m, f, 'table', l))
                elif k == 2: l, j = limits(b, j); imports.append((m, f, 'memory', l))
                elif k == 3: ty = b[j]; mut = b[j+1]; j += 2; imports.append((m, f, 'global', (VT[ty], bool(mut))))
        elif sid == 3:
            funcs, j = uleb(b, j)
        elif sid == 4:
            n, j = uleb(b, j)
            for _ in range(n): ty = b[j]; j += 1; l, j = limits(b, j); tab.append((VT[ty], l))
        elif sid == 5:
            n, j = uleb(b, j)
            for _ in range(n): l, j = limits(b, j); mem.append(l)
        elif sid == 6:
            n, j = uleb(b, j)
            for _ in range(n):
                ty = b[j]; mut = b[j+1]; j += 2; glob.append((VT[ty], bool(mut)))
                while b[j] != 0x0b: j += 1
                j += 1
        elif sid == 7:
            n, j = uleb(b, j)
            for _ in range(n):
                nm, j = name(b, j); k = b[j]; j += 1; idx, j = uleb(b, j); exports.append((nm, ['func','table','memory','global','tag'][k]))
        elif sid == 8: start, j = uleb(b, j)
        elif sid == 9: elem, j = uleb(b, j)
        elif sid == 10: code_range = (j, end)
        elif sid == 11: data, j = uleb(b, j)
        elif sid == 12: pass
        elif sid == 13: out['tag_section'] = True
        i = end
    out.update(types=len(types), imports=imports, funcs=funcs, tables=tab, memories=mem, globals=glob,
               exports=exports, start=start, elem_segments=elem, data_segments=data, custom=custom)
    # feature scan over the code section
    feats = collections.Counter(); j, end = code_range; n, j = uleb(b, j); out['bodies'] = n
    mvp_imm = {}
    for _ in range(n):
        bsz, j = uleb(b, j); bend = j + bsz
        nl, j = uleb(b, j)
        for _ in range(nl): _, j = uleb(b, j); j += 1
        while j < bend:
            op = b[j]; j += 1
            if op in (0x02, 0x03, 0x04): j = skip_blocktype(b, j)
            elif op in (0x25, 0x26): _, j = uleb(b, j); feats['table.get/set (reference types)'] += 1
            elif op == 0xd1: feats['ref.is_null (reference types)'] += 1
            elif 0xc0 <= op <= 0xc4: feats['sign-extension ops'] += 1
            elif op in (0x0c, 0x0d, 0x10, 0x20, 0x21, 0x22, 0x23, 0x24): _, j = uleb(b, j)
            elif op == 0x0e:
                c, j = uleb(b, j)
                for _ in range(c + 1): _, j = uleb(b, j)
            elif op == 0x11:
                _, j = uleb(b, j); t, j = uleb(b, j)
                if t != 0: feats['call_indirect_table>0'] += 1
            elif 0x28 <= op <= 0x3e: _, j = uleb(b, j); _, j = uleb(b, j)
            elif op in (0x3f, 0x40): j += 1; feats['memory.size/grow' if op == 0x3f else 'memory.grow'] += 1
            elif op == 0x41: _, j = sleb(b, j)
            elif op == 0x42: _, j = sleb(b, j)
            elif op == 0x43: j += 4
            elif op == 0x44: j += 8
            elif op == 0xd0: j += 1; feats['ref.null'] += 1
            elif op == 0xd2: _, j = uleb(b, j); feats['ref.func'] += 1
            elif op == 0xfc:
                sub, j = uleb(b, j); feats['0xFC:' + ('sat_trunc' if sub < 8 else 'bulk/table')] += 1
                if sub in (8,): _, j = uleb(b, j); j += 1
                elif sub == 9: _, j = uleb(b, j)
                elif sub == 10: j += 2
                elif sub == 11: j += 1
                elif sub in (12, 14): _, j = uleb(b, j); _, j = uleb(b, j)
                elif sub in (13, 15, 16, 17): _, j = uleb(b, j)
                if sub >= 8: feats['bulk-memory/table ops'] += 1
            elif op == 0xfd:
                feats['SIMD'] += 1; sub, j = uleb(b, j)
                if sub <= 11 or sub in (92, 93): _, j = uleb(b, j); _, j = uleb(b, j)
                elif sub == 12: j += 16
                elif sub == 13: j += 16
                elif 21 <= sub <= 34: j += 1
                elif 84 <= sub <= 91: _, j = uleb(b, j); _, j = uleb(b, j); j += 1
            elif op == 0xfe:
                feats['ATOMICS/threads'] += 1; sub, j = uleb(b, j)
                if sub != 3: _, j = uleb(b, j); _, j = uleb(b, j)
                else: j += 1
            elif op in (0x06, 0x07, 0x08, 0x09, 0x18, 0x19): feats['exceptions/delegate'] += 1
            elif op in (0x12, 0x13): feats['tail calls'] += 1; _, j = uleb(b, j)
            elif op == 0x1c: feats['typed select'] += 1; c, j = uleb(b, j); j += c
        assert j == bend, (path, j, bend)
    out['features'] = dict(feats)
    return out

for p in sys.argv[1:]:
    c = census(p)
    print('==', c['file'], c['bytes'], 'bytes; wasm version', c['version'])
    print('  types', c['types'], 'funcs', c['funcs'], 'bodies', c['bodies'], 'elem segs', c['elem_segments'], 'data segs', c['data_segments'], 'start', c['start'])
    print('  memories', c['memories'], 'tables', c['tables'], 'globals', c['globals'])
    print('  imports:'); [print('    ', x) for x in c['imports']]
    print('  exports:', [e for e in c['exports']])
    print('  features used:', c['features'] or 'none beyond MVP')
    print('  custom sections:', c['custom'])
