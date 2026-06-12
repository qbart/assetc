#include "shader.hpp"

#include "../deps/fmt.hpp"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <set>
#include <vector>

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

// Compose `module` with a single `entryPoint`, link, reflect the entry point's
// name, and write its SPIR-V to `<outDir>/<name>.spv`. `used` tracks names
// already emitted in this folder so collisions fail loud (one folder = one
// pipeline, so duplicate entry-point names would clobber each other). On success
// the emitted name is appended to `emitted`.
int compileEntryPoint(slang::ISession *session, slang::IModule *module,
                      slang::IEntryPoint *entryPoint, const std::string &outDir,
                      std::set<std::string> &used, std::vector<std::string> &emitted)
{
    slang::IComponentType        *components[] = {module, entryPoint};
    ComPtr<slang::IComponentType> program;
    ComPtr<slang::IBlob>          diagnostics;
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

    // Reflect the composed program to recover the entry point's source name; that
    // name (not the stage) becomes the `.spv` stem, so the engine references each
    // entry point by the name it was declared with.
    diagnostics.setNull();
    slang::ProgramLayout *layout = linked->getLayout(0, diagnostics.writeRef());
    if (!layout || layout->getEntryPointCount() == 0)
    {
        reportDiagnostics("reflect", diagnostics.get());
        fmtx::Error("shader: failed to reflect entry point");
        return 1;
    }
    const std::string name = layout->getEntryPointByIndex(0)->getName();

    if (!used.insert(name).second)
    {
        fmtx::Error(fmt::format(
            "shader: duplicate entry-point name '{}' in {} (names must be unique per folder)",
            name, outDir));
        return 1;
    }

    ComPtr<slang::IBlob> spirv;
    diagnostics.setNull();
    if (SLANG_FAILED(linked->getEntryPointCode(0, 0, spirv.writeRef(), diagnostics.writeRef())))
    {
        reportDiagnostics("codegen", diagnostics.get());
        return 1;
    }

    const std::string outFile = (fs::path(outDir) / (name + ".spv")).string();
    std::ofstream     f(outFile, std::ios::binary);
    if (!f)
    {
        fmtx::Error(fmt::format("shader: cannot write {}", outFile));
        return 1;
    }
    f.write(static_cast<const char *>(spirv->getBufferPointer()),
            static_cast<std::streamsize>(spirv->getBufferSize()));

    emitted.push_back(name);
    return 0;
}

} // namespace

int CompileShaderFolder(const std::string &shaderDir, const std::string &outDir,
                        std::vector<std::string> *outEntryPoints)
{
    // Collect the `.slang` files up front and sort them so emission order (and any
    // diagnostics) is deterministic regardless of directory iteration order.
    std::vector<std::string> modules; // module names (stems)
    {
        std::error_code ec;
        for (const auto &e : fs::directory_iterator(shaderDir, ec))
        {
            if (e.is_regular_file() && e.path().extension() == ".slang")
                modules.push_back(e.path().stem().string());
        }
        if (ec)
        {
            fmtx::Error(fmt::format("shader: cannot read {}: {}", shaderDir, ec.message()));
            return 1;
        }
    }
    if (modules.empty())
    {
        fmtx::Error(fmt::format("shader: {} contains no .slang files", shaderDir));
        return 1;
    }
    std::sort(modules.begin(), modules.end());

    // A global session is not thread-safe to share, so each call owns one. This
    // matches the worker-per-asset model in main(): shaders are few and the
    // embedded core module keeps creation cheap.
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

    std::set<std::string>    used;    // entry-point names already emitted (uniqueness)
    std::vector<std::string> emitted; // emitted names, in order
    for (const std::string &moduleName : modules)
    {
        ComPtr<slang::IBlob> diagnostics;
        slang::IModule *module = session->loadModule(moduleName.c_str(), diagnostics.writeRef());
        reportDiagnostics("load", diagnostics.get());
        if (!module)
        {
            fmtx::Error(fmt::format("shader: failed to load {}.slang", moduleName));
            return 1;
        }

        const SlangInt32 count = module->getDefinedEntryPointCount();
        for (SlangInt32 i = 0; i < count; ++i)
        {
            ComPtr<slang::IEntryPoint> entryPoint;
            if (SLANG_FAILED(module->getDefinedEntryPoint(i, entryPoint.writeRef())))
            {
                fmtx::Error(
                    fmt::format("shader: cannot get entry point {} of {}.slang", i, moduleName));
                return 1;
            }
            if (int rc = compileEntryPoint(session.get(), module, entryPoint.get(), outDir, used,
                                           emitted);
                rc != 0)
                return rc;
        }
    }

    if (emitted.empty())
    {
        fmtx::Error(fmt::format(
            "shader: no entry points found in {} (mark them with [shader(\"...\")])", shaderDir));
        return 1;
    }

    if (outEntryPoints)
        *outEntryPoints = std::move(emitted);
    return 0;
}

} // namespace assetc
