package main
import "core:fmt"
import wgpu "vendor:wgpu"
check_integration :: proc() {
    version := wgpu.GetVersion()
    assert(version != 0)
    fmt.printf("Static wgpu-native: ok (version %x)\n", version)
}
