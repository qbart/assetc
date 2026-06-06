# Each test is a standalone executable linked against assetc_core and registered
# with ctest. Add a new test with: assetc_test(test_foo).
function(assetc_test name)
    add_executable(${name} ${name}.cpp)
    set_target_properties(${name} PROPERTIES CXX_STANDARD 23 CXX_EXTENSIONS OFF)
    target_link_libraries(${name} PRIVATE assetc_core)
    target_include_directories(${name} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
    add_test(NAME ${name} COMMAND ${name})
endfunction()

assetc_test(test_hash)
assetc_test(test_manifest)
assetc_test(test_cache)
assetc_test(test_check)
assetc_test(test_anim)
assetc_test(test_mesh)
assetc_test(test_pack)
