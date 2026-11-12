"""Independent finite contract model; no engine imports, FFI, or consumer output."""
import copy
import struct

MAGIC = b'WDBTRC1\n'
# BEGIN, PUT, GET, SCAN, COMMIT, DELETE, RELEASE_TX, SNAPSHOT,
# DROP_SNAPSHOT, RETAIN, OPEN_RETAINED, REOPEN, RELEASE_RETAINED.
TRACE = [
    (1,0,0,0),(2,0,0,1),(2,0,255,2),(5,0,0,0),(7,0,0,0),
    (8,0,0,0),(10,0,0,0),
    # Existing-record writes merge despite sharing their base revision.
    (1,0,0,0),(1,1,0,0),(2,0,0,3),(2,1,255,4),
    (5,0,0,0),(5,1,0,0),(5,1,0,0),(7,0,0,0),(7,1,0,0),
    # Stale point read, terminal failed commit, and read after failure.
    (1,0,0,0),(1,1,0,0),(3,0,0,0),(2,0,255,5),(2,1,0,6),
    (5,1,0,0),(5,0,0,0),(5,0,0,0),(3,0,0,0),(7,0,0,0),(7,1,0,0),
    # Scan observes the whole collection, including an unrelated insert.
    (1,0,0,0),(4,0,0,0),(1,1,0,0),(2,1,7,7),
    (5,1,0,0),(2,0,255,8),(5,0,0,0),(7,0,0,0),(7,1,0,0),
    # Missing reads conservatively observe the collection too.
    (1,0,0,0),(3,0,99,0),(1,1,0,0),(2,1,0,9),
    (5,1,0,0),(6,0,99,0),(5,0,0,0),(7,0,0,0),(7,1,0,0),
    # Missing deletion succeeds; overlay scans include the deletion.
    (1,0,0,0),(6,0,99,0),(6,0,255,0),(3,0,255,0),
    (4,0,0,0),(5,0,0,0),(7,0,0,0),
    (9,0,0,0),(12,0,0,0),(11,0,1,0),
    # Durable ID release preserves a live historical reader.
    (13,0,1,0),(9,0,0,0),(12,0,0,0),(11,0,1,0),
]

def value(n):
    return f'm{{6e:u{n},76:l[b00{n:02x}ff,u{n+1}]}}'

def rows(state):
    return '[' + ';'.join(f'{k:02x}={value(v)}' for k,v in sorted(state.items())) + ']'

def encode(cells=TRACE):
    return MAGIC + struct.pack('<I',len(cells)) + b''.join(struct.pack('<4I',*c) for c in cells)

def decode(data):
    if len(data)<12 or data[:8]!=MAGIC:
        raise ValueError('bad trace magic')
    count, = struct.unpack('<I',data[8:12])
    if count>256 or len(data)!=12+16*count:
        raise ValueError('invalid bounded trace size')
    cells = list(struct.iter_unpack('<4I',data[12:]))
    for op,slot,key,n in cells:
        if not 1<=op<=13 or slot>=4 or key>255 or n>254:
            raise ValueError('invalid trace cell')
    return cells

def expected(cells=TRACE):
    state, versions, txs, retained = {}, {}, {}, {}
    rev = collection_rev = 1 # fresh collection is publication 1
    held = None
    next_id = 1
    lines=[]
    for i,(op,slot,key,n) in enumerate(cells):
        status, result = 0, '-'
        if op==1:
            assert slot not in txs
            txs[slot] = {'state':copy.copy(state),'versions':copy.copy(versions),
                         'cr':collection_rev,'points':{},'scan':False,'writes':{},'closed':False}
        elif op in (2,3,4,5,6):
            t=txs[slot]
            if t['closed']:
                status=11
            elif op==5:
                t['closed']=True
                if (t['scan'] and t['cr']!=collection_rev) or any(versions.get(k)!=v for k,v in t['points'].items()):
                    status=10
                else:
                    rev+=1
                    for k,v in t['writes'].items():
                        collection_rev=rev
                        if v is None:
                            state.pop(k,None); versions.pop(k,None)
                        else:
                            state[k]=v; versions[k]=rev
                    result=str(rev)
            elif op==4:
                t['scan']=True
                overlay={**t['state'],**t['writes']}
                result=rows({k:v for k,v in overlay.items() if v is not None})
            else:
                if key in t['versions']:
                    t['points'][key]=t['versions'][key]
                else:
                    t['scan']=True
                if op==2: t['writes'][key]=n
                if op==6: t['writes'][key]=None
                if op==3:
                    v=t['writes'].get(key,t['state'].get(key))
                    if v is None: status=3
                    else: result=value(v)
        elif op==7:
            del txs[slot]
        elif op==8:
            assert held is None
            held=(rev,copy.copy(state))
        elif op==9:
            assert held is not None
            held=None
        elif op==10:
            assert held is not None
            retained[next_id]=copy.deepcopy(held)
            result=str(next_id); next_id+=1; rev+=1
        elif op==11:
            assert held is None
            if key not in retained: status=3
            else: held=copy.deepcopy(retained[key])
        elif op==12:
            assert not txs and held is None
        elif op==13:
            if key not in retained: status=3
            else: del retained[key]; rev+=1
        h='-' if held is None else str(held[0])+rows(held[1])
        r=','.join(map(str, sorted(retained))) or '-'
        lines.append(f'{i} {op} {status} {rev} {rows(state)} H={h} R={r} X={result}')
    assert not txs and held is None
    return '\n'.join(lines)+'\n'
