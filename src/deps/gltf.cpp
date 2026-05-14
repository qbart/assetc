#include "gltf.hpp"
#include "fmt.hpp"
#include <memory>

gltf::GLTF::~GLTF() { tg3_model_free(&model); }

std::unique_ptr<gltf::GLTF> gltf::Load(const std::string &path)
{

    tg3_parse_options opts;
    tg3_error_stack errors;

    tg3_parse_options_init(&opts);
    tg3_error_stack_init(&errors);
    auto g = std::make_unique<gltf::GLTF>();

    tg3_error_code err = tg3_parse_file(&g->model, &errors, path.c_str(), 255, &opts);

    if (err != TG3_OK)
    {
        for (uint32_t i = 0; i < errors.count; i++)
        {
            fmtx::Error(
                fmt::format(
                    "[{}] {}",
                    (int)errors.entries[i].severity,
                    errors.entries[i].message ? errors.entries[i].message : "(null)"
                )
            );
        }

        return nullptr;
    }
    tg3_error_stack_free(&errors);

    return std::move(g);
}
