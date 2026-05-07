#pragma once

#include <fmt/core.h>
#include <fmt/format.h>
#include <string>

namespace fmtx
{
constexpr static auto RESET  = "\033[0m";
constexpr static auto RED    = "\033[0;31m";
constexpr static auto YELLOW = "\033[0;33m";
constexpr static auto BLUE   = "\033[0;34m";
constexpr static auto GREEN  = "\033[0;32m";
constexpr static auto CYAN   = "\033[0;36m";

void Warn(const std::string &s);
void Error(const std::string &s);
void Info(const std::string &s);
void Debug(const std::string &s);
void Success(const std::string &s);
} // namespace fmtx
