SUMMARY = "OneKVM RustDesk ID, relay, and direct-IP protocol extension"
LICENSE = "AGPL-3.0-only"
LIC_FILES_CHKSUM = "file://LICENSE;md5=4ae09d45eac4aa08d013b5f2e01c67f6"
PV = "0.1.17"
PR = "r4"

ONEKVM_COMPONENT_GIT_URI ??= "git://github.com/onekvm/onekvm-extension-rustdesk.git;protocol=https;branch=main"
ONEKVM_COMPONENT_SRCREV ??= ""
SRC_URI = "${ONEKVM_COMPONENT_GIT_URI}"
SRCREV = "${ONEKVM_COMPONENT_SRCREV}"

require onekvm-extension-rustdesk-crates.inc

inherit cargo cargo-update-recipe-crates onekvm-extension

ONEKVM_EXTENSION_ID = "rustdesk"
ONEKVM_EXTENSION_API_VERSION = "2"
ONEKVM_EXTENSION_MANIFEST_TEMPLATE = "${S}/manifest.json.in"
RDEPENDS:${PN} += "onekvm-core (>= 0.1.0-r1)"

do_install() {
    oe_runmake -C ${S} install \
        BINARY=${B}/target/${RUST_HOST_SYS}/${BUILD_DIR}/onekvm-protocol-rustdesk \
        DESTDIR=${D} PREFIX=${ONEKVM_EXTENSION_ROOT} \
        VERSION=${PV} ARCH=${TARGET_ARCH}
}
