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

// Canonical lowercase name for a shader stage; used as the emitted name when an
// entry point is the conventional `main` (the stage is the only distinguishing
// information in that case).
std::string_view stageName(SlangStage stage)
{
    switch (stage)
    {
    case SLANG_STAGE_VERTEX:        return "vertex";
    case SLANG_STAGE_FRAGMENT:      return "fragment";
    case SLANG_STAGE_COMPUTE:       return "compute";
    case SLANG_STAGE_GEOMETRY:      return "geometry";
    case SLANG_STAGE_HULL:          return "hull";
    case SLANG_STAGE_DOMAIN:        return "domain";
    case SLANG_STAGE_MESH:          return "mesh";
    case SLANG_STAGE_AMPLIFICATION: return "amplification";
    case SLANG_STAGE_RAY_GENERATION:return "raygen";
    case SLANG_STAGE_INTERSECTION:  return "intersection";
    case SLANG_STAGE_ANY_HIT:       return "anyhit";
    case SLANG_STAGE_CLOSEST_HIT:   return "closesthit";
    case SLANG_STAGE_MISS:          return "miss";
    case SLANG_STAGE_CALLABLE:      return "callable";
    default:                        return "";
    }
}

bool equalsIgnoreCase(std::string_view a, std::string_view b)
{
    if (a.size() != b.size()) return false;
    for (size_t i = 0; i < a.size(); ++i)
    {
        char ca = a[i], cb = b[i];
        if (ca >= 'A' && ca <= 'Z') ca = static_cast<char>(ca - 'A' + 'a');
        if (cb >= 'A' && cb <= 'Z') cb = static_cast<char>(cb - 'A' + 'a');
        if (ca != cb) return false;
    }
    return true;
}

// Map a reflected entry-point name + stage to the `.spv` stem the asset emits:
//   - `main` (any case)        -> the stage name        (vertex, fragment, ...)
//   - a trailing exact "Main"  -> the name without it    (vsMain -> vs)
//   - anything else            -> the name unchanged     (down -> down)
// Capital-M-only stripping avoids mangling words like `domain`.
std::string emittedName(std::string_view reflected, SlangStage stage)
{
    if (equalsIgnoreCase(reflected, "main"))
        return std::string(stageName(stage));
    constexpr std::string_view kSuffix = "Main";
    if (reflected.size() > kSuffix.size() && reflected.ends_with(kSuffix))
        return std::string(reflected.substr(0, reflected.size() - kSuffix.size()));
    return std::string(reflected);
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

    // Reflect the composed program to recover the entry point's source name and
    // stage, then derive the `.spv` stem (see emittedName): `main` becomes the
    // stage name, a `…Main` suffix is dropped, otherwise the name is kept as-is.
    diagnostics.setNull();
    slang::ProgramLayout *layout = linked->getLayout(0, diagnostics.writeRef());
    if (!layout || layout->getEntryPointCount() == 0)
    {
        reportDiagnostics("reflect", diagnostics.get());
        fmtx::Error("shader: failed to reflect entry point");
        return 1;
    }
    slang::EntryPointReflection *refl = layout->getEntryPointByIndex(0);
    const std::string            name = emittedName(refl->getName(), refl->getStage());
    if (name.empty())
    {
        fmtx::Error(fmt::format("shader: entry point '{}' has no name and an unknown stage",
                                refl->getName()));
        return 1;
    }

    if (!used.insert(name).second)
    {
        fmtx::Error(fmt::format(
            "shader: entry point '{}' maps to '{}', which already exists in {} "
            "(emitted names must be unique per folder)",
            refl->getName(), name, outDir));
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
