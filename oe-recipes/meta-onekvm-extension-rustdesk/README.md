# OpenEmbedded layer

This layer packages the RustDesk extension for OneKVM. The integrating distro
must set `ONEKVM_COMPONENT_SRCREV`. The layer supports `wrynose` and depends
on the distro-owned `onekvm` layer.
