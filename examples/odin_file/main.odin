package main
import "core:fmt"
import "core:os"
import "core:c"
import wgpu "vendor:wgpu"
ENGINE_LIB :: #config(ENGINE_LIB,"../../target/debug/libweekend_db.dylib")
foreign import engine { ENGINE_LIB }
Db :: distinct rawptr
Value :: distinct rawptr
Tx :: distinct rawptr
Snapshot :: distinct rawptr
foreign engine {
    wdb_file_create :: proc(path:[^]u8,len:c.size_t,out:^Db)->i32 ---
    wdb_file_open :: proc(path:[^]u8,len:c.size_t,out:^Db)->i32 ---
    wdb_file_compact :: proc(path:[^]u8,len:c.size_t,before,after:^u64)->i32 ---
    wdb_close :: proc(db:Db)->i32 ---
    wdb_collection_create :: proc(db:Db,out:^u64)->i32 ---
    wdb_value_make :: proc(db:Db,kind:i32,bits:u64,bytes:[^]u8,len:c.size_t,out:^Value)->i32 ---
    wdb_value_scalar :: proc(value:Value,out:^u64)->i32 ---
    wdb_value_release :: proc(value:Value)->i32 ---
    wdb_tx_begin :: proc(db:Db,validation:i32,out:^Tx)->i32 ---
    wdb_tx_put :: proc(tx:Tx,collection:u64,key:[^]u8,len:c.size_t,value:Value)->i32 ---
    wdb_tx_commit :: proc(tx:Tx,out:^u64)->i32 ---
    wdb_tx_release :: proc(tx:Tx)->i32 ---
    wdb_snapshot_acquire :: proc(db:Db,out:^Snapshot)->i32 ---
    wdb_snapshot_retain :: proc(db:Db,snapshot:Snapshot,out:^u64)->i32 ---
    wdb_snapshot_release :: proc(snapshot:Snapshot)->i32 ---
    wdb_snapshot_get :: proc(snapshot:Snapshot,collection:u64,key:[^]u8,len:c.size_t,out:^Value)->i32 ---
    wdb_retained_open :: proc(db:Db,id:u64,out:^Snapshot)->i32 ---
    wdb_retained_release :: proc(db:Db,id:u64)->i32 ---
}
main :: proc() {
    assert(len(os.args)==2)
    path:=os.args[1]
    db:Db
    assert(wdb_file_create(raw_data(path),c.size_t(len(path)),&db)==0)
    collection,revision,id:u64
    assert(wdb_collection_create(db,&collection)==0)
    one,two:Value
    assert(wdb_value_make(db,3,1,nil,0,&one)==0)
    assert(wdb_value_make(db,3,2,nil,0,&two)==0)
    tx:Tx
    key:="a"
    assert(wdb_tx_begin(db,0,&tx)==0)
    assert(wdb_tx_put(tx,collection,raw_data(key),1,one)==0)
    assert(wdb_tx_commit(tx,&revision)==0)
    assert(wdb_tx_release(tx)==0)
    old:Snapshot
    assert(wdb_snapshot_acquire(db,&old)==0)
    assert(wdb_snapshot_retain(db,old,&id)==0)
    assert(wdb_tx_begin(db,0,&tx)==0)
    assert(wdb_tx_put(tx,collection,raw_data(key),1,two)==0)
    assert(wdb_tx_commit(tx,&revision)==0)
    assert(wdb_tx_release(tx)==0)
    assert(wdb_snapshot_release(old)==0)
    assert(wdb_value_release(one)==0)
    assert(wdb_value_release(two)==0)
    assert(wdb_close(db)==0)
    before,after:u64
    assert(wdb_file_compact(raw_data(path),c.size_t(len(path)),&before,&after)==0 && after<=before)
    assert(wdb_file_open(raw_data(path),c.size_t(len(path)),&db)==0)
    assert(wdb_retained_open(db,id,&old)==0)
    got:Value
    bits:u64
    assert(wdb_snapshot_get(old,collection,raw_data(key),1,&got)==0)
    assert(wdb_value_scalar(got,&bits)==0 && bits==1)
    assert(wdb_value_release(got)==0)
    assert(wdb_retained_release(db,id)==0)
    assert(wdb_snapshot_release(old)==0)
    assert(wdb_close(db)==0)
    version:=wgpu.GetVersion()
    assert(version!=0)
    fmt.printf("Odin file/shared-library consumer + static wgpu-native: passed (%x)\n",version)
}
