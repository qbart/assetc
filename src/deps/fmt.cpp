#include "fmt.hpp"
#include <fmt/color.h>
#include <fmt/printf.h>

namespace fmtx
{
void Warn(const std::string &s) { fmt::print("{}[warn] {}{}\n", YELLOW, RESET, s); }

void Error(const std::string &s) { fmt::print("{}[error] {}{}\n", RED, RESET, s); }

void Info(const std::string &s) { fmt::print("{}[info] {}{}\n", BLUE, RESET, s); }

void Debug(const std::string &s) { fmt::print("{}[debug] {}{}\n", CYAN, RESET, s); }

void Success(const std::string &s) { fmt::print("{}[ok] {}{}\n", GREEN, RESET, s); }
} // namespace fmtx
