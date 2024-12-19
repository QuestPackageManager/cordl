# cordl

Requires Ubuntu 22.04 (lib6 requirement)

While this may generate headers for Android IL2CPP (or any future supported platforms), you are highly suggested to run this under WSL for convenience.

`libclang` is required for building the binary. 

# Running

Find the `global-metadata.dat` and `libil2cpp.so` in your IL2CPP binary and run:
```
cargo run --features il2cpp_v31 --metadata ./global-metadata.dat --libil2cpp ./libil2cpp.so cpp
```

Change `cpp` to the target generation of your choosing. Use the `json` target if you wish to use it for other means.