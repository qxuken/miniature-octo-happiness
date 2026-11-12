package main
import "core:c"
import "core:fmt"
import "core:os"
import db "weekend:odin"

#assert(!ODIN_DISABLE_ASSERT)

ok :: proc(status: db.Status) { assert(status == db.WDB_OK) }
hex :: proc(bytes: []u8) { for b in bytes { fmt.printf("%02x", b) } }
print_scan :: proc(s: db.Scan, map_value: bool) {
    count: c.size_t
    ok(db.wdb_scan_len(s, &count)); assert(count <= 16)
    if map_value {fmt.print("m{")} else {fmt.print("[")}
    for i in 0..<count {
        key: [32]u8
        need: c.size_t
        assert(db.wdb_scan_key(s,i,nil,0,&need)==db.WDB_BUFFER && need>0 && need<=32)
        initial:=need
        for &b in key[:] {b=0xa5}
        need=99
        assert(db.wdb_scan_key(s,i,raw_data(key[:]),initial-1,&need)==db.WDB_BUFFER && need==initial)
        for b in key[:] {assert(b==0xa5)}
        need=99;ok(db.wdb_scan_key(s,i,raw_data(key[:]),32,&need));assert(need==initial)
        if i>0 {if map_value {fmt.print(",")} else {fmt.print(";")}}
        hex(key[:int(need)])
        if map_value {fmt.print(":")} else {fmt.print("=")}
        v: db.Value
        ok(db.wdb_scan_value(s,i,&v)); print_value(v); ok(db.wdb_value_release(v))
    }
    if map_value {fmt.print("}")} else {fmt.print("]")}
}
print_value :: proc(v: db.Value) {
    kind: i32
    ok(db.wdb_value_kind(v,&kind))
    switch kind {
    case db.WDB_UNSIGNED:
        n: u64
        ok(db.wdb_value_scalar(v,&n));fmt.printf("u%d",n)
    case db.WDB_BYTES:
        bytes: [32]u8
        n: c.size_t
        assert(db.wdb_value_bytes(v,nil,0,&n)==db.WDB_BUFFER && n>0 && n<=32)
        initial:=n
        for &b in bytes[:] {b=0xa5}
        n=99
        assert(db.wdb_value_bytes(v,raw_data(bytes[:]),initial-1,&n)==db.WDB_BUFFER && n==initial)
        for b in bytes[:] {assert(b==0xa5)}
        n=99;ok(db.wdb_value_bytes(v,raw_data(bytes[:]),32,&n));assert(n==initial);fmt.print("b");hex(bytes[:int(n)])
    case db.WDB_MAP:
        s: db.Scan
        ok(db.wdb_map_scan(v,&s));print_scan(s,true);ok(db.wdb_scan_release(s))
    case db.WDB_LIST:
        n: c.size_t
        ok(db.wdb_value_len(v,&n));assert(n<=16);fmt.print("l[")
        for i in 0..<n {
            x: db.Value
            if i>0 {fmt.print(",")}
            ok(db.wdb_list_get(v,i,&x));print_value(x);ok(db.wdb_value_release(x))
        }
        fmt.print("]")
    case: panic("unexpected value kind")
    }
}
make_value :: proc(database: db.Database,n: u32) -> db.Value {
    u,b,list,next,map_value,result: db.Value
    bytes := [3]u8{0,u8(n),255}
    ok(db.wdb_value_make(database,db.WDB_UNSIGNED,u64(n)+1,nil,0,&u))
    ok(db.wdb_value_make(database,db.WDB_BYTES,0,raw_data(bytes[:]),3,&b))
    ok(db.wdb_value_make(database,db.WDB_LIST,0,nil,0,&list))
    ok(db.wdb_list_push(list,b,&next));ok(db.wdb_value_release(list));list=next
    ok(db.wdb_list_push(list,u,&next));ok(db.wdb_value_release(list));list=next
    ok(db.wdb_value_release(u));ok(db.wdb_value_release(b))
    ok(db.wdb_value_make(database,db.WDB_MAP,0,nil,0,&map_value))
    ok(db.wdb_value_make(database,db.WDB_UNSIGNED,u64(n),nil,0,&u))
    nk,vk := "n","v"
    ok(db.wdb_map_set(map_value,raw_data(nk),1,u,&next));ok(db.wdb_value_release(map_value));map_value=next
    ok(db.wdb_map_set(map_value,raw_data(vk),1,list,&result))
    ok(db.wdb_value_release(map_value));ok(db.wdb_value_release(list));ok(db.wdb_value_release(u))
    return result
}
read32 :: proc(bytes: []u8) -> u32 {
    assert(len(bytes)>=4)
    return u32(bytes[0]) | u32(bytes[1])<<8 | u32(bytes[2])<<16 | u32(bytes[3])<<24
}
main :: proc() {
    assert(len(os.args)==4)
    bytes,err := os.read_entire_file(os.args[1],context.allocator)
    assert(err==nil);defer delete(bytes)
    assert(len(bytes)>=12 && len(bytes)<=4108 && string(bytes[:8])=="WDBTRC1\n")
    count:=int(read32(bytes[8:12]));assert(count<=256 && len(bytes)==12+16*count)
    identity: [256]u8
    required: c.size_t
    ok(db.wdb_source_id(raw_data(identity[:]),256,&required));assert(string(identity[:int(required)])==os.args[3])
    assert(db.wdb_abi_version()==db.WDB_ABI_VERSION && db.wdb_format_version()==db.WDB_FORMAT_VERSION)
    path:=os.args[2]
    database: db.Database
    ok(db.wdb_file_create(raw_data(path),c.size_t(len(path)),&database))
    collection: u64
    ok(db.wdb_collection_create(database,&collection));assert(collection==1)
    tx: [4]db.Transaction
    held: db.Snapshot
    for i in 0..<count {
        cell:=bytes[12+i*16:12+(i+1)*16]
        op,slot,k,n:=read32(cell),int(read32(cell[4:])),read32(cell[8:]),read32(cell[12:])
        assert(op>=1 && op<=13 && slot<4 && k<=255 && n<=254)
        key:=[1]u8{u8(k)}
        status: db.Status
        result: u64 = 0xdead_beef_cafe_babe
        got: db.Value
        got_scan: db.Scan
        switch op {
        case 1: assert(tx[slot]==nil);status=db.wdb_tx_begin(database,db.WDB_RECORDS,&tx[slot])
        case 2:
            assert(tx[slot]!=nil);v:=make_value(database,n)
            status=db.wdb_tx_put(tx[slot],collection,raw_data(key[:]),1,v);ok(db.wdb_value_release(v))
        case 3:
            assert(tx[slot]!=nil)
            probe: db.Value
            ok(db.wdb_value_make(database,db.WDB_UNSIGNED,123,nil,0,&probe));got=probe
            status=db.wdb_tx_get(tx[slot],collection,raw_data(key[:]),1,&got)
            if status!=0 {assert(got==nil)} else {assert(got!=nil && got!=probe)}
            ok(db.wdb_value_release(probe))
        case 4: assert(tx[slot]!=nil);status=db.wdb_tx_scan(tx[slot],collection,&got_scan)
        case 5:
            assert(tx[slot]!=nil);status=db.wdb_tx_commit(tx[slot],&result)
            if status!=0 {assert(result==0)}
        case 6: assert(tx[slot]!=nil);status=db.wdb_tx_delete(tx[slot],collection,raw_data(key[:]),1)
        case 7: assert(tx[slot]!=nil);status=db.wdb_tx_release(tx[slot]);tx[slot]=nil
        case 8: assert(held==nil);status=db.wdb_snapshot_acquire(database,&held)
        case 9: assert(held!=nil);status=db.wdb_snapshot_release(held);held=nil
        case 10: assert(held!=nil);status=db.wdb_snapshot_retain(database,held,&result)
        case 11:
            assert(held==nil)
            probe: db.Snapshot
            ok(db.wdb_snapshot_acquire(database,&probe));held=probe
            status=db.wdb_retained_open(database,u64(k),&held)
            if status!=0 {assert(held==nil)} else {assert(held!=nil && held!=probe)}
            ok(db.wdb_snapshot_release(probe))
        case 12:
            assert(held==nil);for t in tx {assert(t==nil)}
            ok(db.wdb_close(database));status=db.wdb_file_open(raw_data(path),c.size_t(len(path)),&database)
        case 13: status=db.wdb_retained_release(database,u64(k))
        }
        current: db.Snapshot
        revision: u64
        s: db.Scan
        ok(db.wdb_snapshot_acquire(database,&current));ok(db.wdb_snapshot_revision(current,&revision))
        fmt.printf("%d %d %d %d ",i,op,status,revision)
        ok(db.wdb_snapshot_scan(current,collection,&s));ok(db.wdb_snapshot_release(current));print_scan(s,false);ok(db.wdb_scan_release(s))
        fmt.print(" H=")
        if held!=nil {
            ok(db.wdb_snapshot_revision(held,&revision));fmt.printf("%d",revision)
            ok(db.wdb_snapshot_scan(held,collection,&s));print_scan(s,false);ok(db.wdb_scan_release(s))
        } else {fmt.print("-")}
        ids: [16]u64
        length: c.size_t
        probe:=db.wdb_retained_ids(database,nil,0,&length)
        assert((probe==db.WDB_BUFFER && length>0)||(probe==db.WDB_OK && length==0));assert(length<=16)
        ok(db.wdb_retained_ids(database,raw_data(ids[:]),16,&length));fmt.print(" R=")
        if length==0 {fmt.print("-")}
        for j in 0..<int(length) {if j>0 {fmt.print(",")};fmt.printf("%d",ids[j])}
        fmt.print(" X=")
        if got!=nil {print_value(got);ok(db.wdb_value_release(got))}
        else if got_scan!=nil {print_scan(got_scan,false);ok(db.wdb_scan_release(got_scan))}
        else if status==0 && (op==5 || op==10) {fmt.printf("%d",result)}
        else {fmt.print("-")}
        fmt.println()
    }
    assert(held==nil);for t in tx {assert(t==nil)};ok(db.wdb_close(database))
}
