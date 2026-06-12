#include "diag.hpp"

#include <cstdio>
#include <string>

namespace
{
constexpr auto RESET  = "\033[0m";
constexpr auto RED    = "\033[0;31m";
constexpr auto YELLOW = "\033[0;33m";
constexpr auto BLUE   = "\033[0;34m";
constexpr auto GREEN  = "\033[0;32m";

void Line(std::FILE *to, const char *color, const char *tag, std::string_view s)
{
    // One write per call; std::string_view is not guaranteed null-terminated, so
    // pass the length explicitly via the precision specifier.
    std::fprintf(to, "%s%s %s%.*s\n", color, tag, RESET, static_cast<int>(s.size()), s.data());
}
} // namespace

namespace assetc::diag
{
void Error(std::string_view s) { Line(stderr, RED, "[error]", s); }
void Warn(std::string_view s) { Line(stderr, YELLOW, "[warn]", s); }
void Info(std::string_view s) { Line(stdout, BLUE, "[info]", s); }
void Success(std::string_view s) { Line(stdout, GREEN, "[ok]", s); }
} // namespace assetc::diag
