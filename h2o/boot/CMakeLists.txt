if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo xbuild --target x86_64-unknown-uefi)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/x86_64-unknown-uefi/debug)
else ()
    set(CARGO_CMD cargo xbuild --target x86_64-unknown-uefi)
    set(GLOB_PATH ${CMAKE_CURRENT_BINARY_DIR}/x86_64-unknown-uefi/rls)
endif ()

add_custom_target(
    H2O_BOOT ALL
    CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} --manifest-path ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
)

file(GLOB RS_EFI ${GLOB_PATH}/h2o_boot.efi)

add_dependencies(GEN_IMG H2O_BOOT)
add_custom_command(
    TARGET H2O_BOOT POST_BUILD
    COMMAND cp ${RS_EFI} ${CMAKE_SOURCE_DIR}/target/BootX64.efi
)
