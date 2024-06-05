# vhost-device-gpu - GPU emulation backend daemon

## Synopsis
```shell
       vhost-device-gpu --socket-path <SOCKET>
```

## Description
       A virtio-gpu device using the vhost-user protocol.

## Options

```text
       -s, --socket-path <SOCKET>
              vhost-user Unix domain socket path

       -h, --help
              Print help

       -V, --version
              Print version
```

## Examples

First start the daemon on the host machine:

```shell
host# vhost-device-gpu --socket-path /tmp/gpu.socket
```

With QEMU, there are two ways you can add a `virtio` device 
that uses the backend's socket and any GPU display option 
of your choice. The first method is using `vhost-user-gpu-pci`, 
and the second method is using `vhost-user-vga`. 
By default, QEMU display uses VGA, so to use any of the 
virtio device options, ensure that the `vga` option is disabled.

1) Using `vhost-user-gpu-pci` Start QEMU with the following flags:

```text
-chardev socket,id=vgpu,path=/tmp/gpu.socket \
-device vhost-user-gpu-pci,chardev=vgpu,id=vgpu \
-object memory-backend-memfd,share=on,id=mem0,size=4G, \
-machine q35,memory-backend=mem0,accel=kvm \
-display gtk,gl=on,show-cursor=on \
-vga none
```

2) Using `vhost-user-vga` Start QEMU with the following flags:

```text
-chardev socket,id=vgpu,path=/tmp/gpu.socket \
-device vhost-user-vga,chardev=vgpu,id=vgpu \
-object memory-backend-memfd,share=on,id=mem0,size=4G, \
-machine q35,memory-backend=mem0,accel=kvm \
-display gtk,gl=on,show-cursor=on \
-vga none
```

## License

This project is licensed under either of

- [Apache License](http://www.apache.org/licenses/LICENSE-2.0), Version 2.0
- [BSD-3-Clause License](https://opensource.org/licenses/BSD-3-Clause)
