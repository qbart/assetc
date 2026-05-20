#include "shader.hpp"

#include "../deps/fmt.hpp"

#include <filesystem>
#include <fstream>

#include <slang-com-ptr.h>
#include <slang.h>

namespace fs = std::filesystem;
using Slang::ComPtr;

namespace assetc
{
namespace
{

// Print a Slang diagnostics blob if it carries any text. Slang writes both
// warnings and errors here; we surface everything so build output is useful.
void reportDiagnostics(std::string_view what, slang::IBlob *diagnostics)
{
    if (diagnostics && diagnostics->getBufferSize() > 0)
        fmtx::Warn(fmt::format(
            "slang {}: {}", what, static_cast<const char *>(diagnostics->getBufferPointer())));
}

// Compile one stage. `moduleName` resolves to `<moduleName>.slang` on the
// session search path; the `main` entry point is compiled for `stage` and the
// resulting SPIR-V is written to `outFile`.
int compileStage(slang::ISession *session, const char *moduleName, SlangStage stage,
                 const std::string &outFile)
{
    ComPtr<slang::IBlob> diagnostics;

    slang::IModule *module = session->loadModule(moduleName, diagnostics.writeRef());
    reportDiagnostics("load", diagnostics.get());
    if (!module)
    {
        fmtx::Error(fmt::format("shader: failed to load {}.slang", moduleName));
        return 1;
    }

    ComPtr<slang::IEntryPoint> entryPoint;
    diagnostics.setNull();
    if (SLANG_FAILED(module->findAndCheckEntryPoint("main", stage, entryPoint.writeRef(),
                                                    diagnostics.writeRef())))
    {
        reportDiagnostics("entry", diagnostics.get());
        fmtx::Error(fmt::format("shader: no 'main' entry point in {}.slang", moduleName));
        return 1;
    }

    slang::IComponentType        *components[] = {module, entryPoint.get()};
    ComPtr<slang::IComponentType> program;
    diagnostics.setNull();
    if (SLANG_FAILED(session->createCompositeComponentType(components, 2, program.writeRef(),
                                                           diagnostics.writeRef())))
    {
        reportDiagnostics("compose", diagnostics.get());
        return 1;
    }

    ComPtr<slang::IComponentType> linked;
    diagnostics.setNull();
    if (SLANG_FAILED(program->link(linked.writeRef(), diagnostics.writeRef())))
    {
        reportDiagnostics("link", diagnostics.get());
        return 1;
    }

    ComPtr<slang::IBlob> spirv;
    diagnostics.setNull();
    if (SLANG_FAILED(linked->getEntryPointCode(0, 0, spirv.writeRef(), diagnostics.writeRef())))
    {
        reportDiagnostics("codegen", diagnostics.get());
        return 1;
    }

    std::ofstream f(outFile, std::ios::binary);
    if (!f)
    {
        fmtx::Error(fmt::format("shader: cannot write {}", outFile));
        return 1;
    }
    f.write(static_cast<const char *>(spirv->getBufferPointer()),
            static_cast<std::streamsize>(spirv->getBufferSize()));
    return 0;
}

} // namespace

int CompileShaderFolder(const std::string &shaderDir, const std::string &outDir)
{
    if (!fs::exists(fs::path(shaderDir) / "vertex.slang") ||
        !fs::exists(fs::path(shaderDir) / "fragment.slang"))
    {
        fmtx::Error(
            fmt::format("shader: {} must contain vertex.slang and fragment.slang", shaderDir));
        return 1;
    }

    // A global session is not thread-safe to share, so each call owns one.
    // This matches the worker-per-asset model in main(): shaders are few and
    // the embedded core module keeps creation cheap.
    ComPtr<slang::IGlobalSession> globalSession;
    if (SLANG_FAILED(slang::createGlobalSession(globalSession.writeRef())))
    {
        fmtx::Error("shader: failed to create slang global session");
        return 1;
    }

    slang::TargetDesc target{};
    target.format  = SLANG_SPIRV;
    target.profile = globalSession->findProfile("spirv_1_5");

    // Resolve `import`/`#include` and the stage files relative to the folder.
    const char *searchPaths[] = {shaderDir.c_str()};

    slang::SessionDesc sd{};
    sd.targets         = &target;
    sd.targetCount     = 1;
    sd.searchPaths     = searchPaths;
    sd.searchPathCount = 1;

    ComPtr<slang::ISession> session;
    if (SLANG_FAILED(globalSession->createSession(sd, session.writeRef())))
    {
        fmtx::Error("shader: failed to create slang session");
        return 1;
    }

    fs::create_directories(outDir);
    if (int rc = compileStage(session.get(), "vertex", SLANG_STAGE_VERTEX,
                              (fs::path(outDir) / "vertex.spv").string());
        rc != 0)
        return rc;
    if (int rc = compileStage(session.get(), "fragment", SLANG_STAGE_FRAGMENT,
                              (fs::path(outDir) / "fragment.spv").string());
        rc != 0)
        return rc;
    return 0;
}

} // namespace assetc
