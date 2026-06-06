#pragma once

// Minimal zero-dependency test helper: each test is its own executable with an
// int main(). Use CHECK / CHECK_EQ / CHECK_NEAR to assert, then `return
// test::summary()`. A nonzero exit code marks the ctest case as failed.

#include <cmath>
#include <cstdio>
#include <string>

namespace test
{
inline int g_checks   = 0;
inline int g_failures = 0;

inline int summary()
{
    std::fprintf(stderr, "%d/%d checks passed\n", g_checks - g_failures, g_checks);
    return g_failures == 0 ? 0 : 1;
}
} // namespace test

#define CHECK(cond)                                                                                \
    do                                                                                             \
    {                                                                                              \
        ++test::g_checks;                                                                          \
        if (!(cond))                                                                               \
        {                                                                                          \
            ++test::g_failures;                                                                    \
            std::fprintf(stderr, "  FAIL %s:%d: CHECK(%s)\n", __FILE__, __LINE__, #cond);          \
        }                                                                                          \
    } while (0)

#define CHECK_EQ(a, b)                                                                             \
    do                                                                                             \
    {                                                                                              \
        ++test::g_checks;                                                                          \
        auto _va = (a);                                                                            \
        auto _vb = (b);                                                                            \
        if (!(_va == _vb))                                                                         \
        {                                                                                          \
            ++test::g_failures;                                                                    \
            std::fprintf(stderr, "  FAIL %s:%d: CHECK_EQ(%s, %s)\n", __FILE__, __LINE__, #a, #b);  \
        }                                                                                          \
    } while (0)

#define CHECK_NEAR(a, b, eps)                                                                      \
    do                                                                                             \
    {                                                                                              \
        ++test::g_checks;                                                                          \
        if (std::fabs(double((a)) - double((b))) > double(eps))                                    \
        {                                                                                          \
            ++test::g_failures;                                                                    \
            std::fprintf(stderr, "  FAIL %s:%d: CHECK_NEAR(%s, %s)\n", __FILE__, __LINE__, #a,     \
                         #b);                                                                      \
        }                                                                                          \
    } while (0)
