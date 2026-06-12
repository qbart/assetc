#pragma once

#include <string_view>

// SDK-internal diagnostics. The loaders report failures through these instead of
// any third-party logging library, so the SDK stays dependency-free (std-only).
// Messages go to stderr (errors/warnings) or stdout (info/success) with the same
// ANSI tags the rest of assetc uses. Implemented in diag.cpp.
namespace assetc::diag
{
void Error(std::string_view s);
void Warn(std::string_view s);
void Info(std::string_view s);
void Success(std::string_view s);
} // namespace assetc::diag
