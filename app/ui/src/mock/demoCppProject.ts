/**
 * A small CMake project for browser mode, opened with `?project=cpp`: `shapes`, a C++20
 * geometry library with a command line app, GoogleTest tests (`TEST`, `TEST_F`, `TEST_P`)
 * and Catch2 tests (`TEST_CASE`, `SECTION`), CMake presets and a `.clang-format`.
 * cppMock.ts answers the `cmake_*` / `cpp_*` commands, running the Rust crates through
 * WebAssembly. `?build=broken` makes the demo build fail with compiler errors.
 */
import type { ProjectInfo } from "@/ipc/project";
import { pioDemoFiles, pioDemoName } from "./demoPioProject";

const demoParam = () => (typeof location !== "undefined" ? new URLSearchParams(location.search).get("project") : null);

/** `?project=pio`: the PlatformIO demo `blinky` (demoPioProject.ts) instead of `shapes`. */
export const isPioDemo = (): boolean => demoParam() === "pio";

export const cppDemoName = isPioDemo() ? pioDemoName : "shapes";

/** True when the page asks for the C++ demo project (or the PlatformIO one). */
export const isCppDemo = (): boolean => demoParam() === "cpp" || demoParam() === "pio";

/** `?build=broken`: the build fails with compiler and linker errors. */
export const cppBrokenBuild = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("build") === "broken";

export const cppDemoInfo: ProjectInfo = {
  languages: ["cpp"],
  frameworks: [],
  packs: ["cforge"],
  tools: isPioDemo() ? ["git", "platformio"] : ["git", "cmake", "vcpkg"],
  primary: { id: "cpp", name: "C/C++", language: "cpp" },
};

const CMAKE = `cmake_minimum_required(VERSION 3.24)
project(shapes VERSION 1.2.0 DESCRIPTION "Polygon geometry" LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(SHAPES_TESTS "Build the tests" ON)

add_library(shapes STATIC
    src/geometry/polygon.cpp
    src/geometry/polygon.hpp
    src/stats.cpp
    src/stats.hpp)
target_include_directories(shapes PUBLIC src)
target_compile_options(shapes PRIVATE -Wall -Wextra)

add_executable(shapes_app src/main.cpp)
target_link_libraries(shapes_app PRIVATE shapes)

if(SHAPES_TESTS)
    include(FetchContent)
    FetchContent_Declare(googletest
        URL https://github.com/google/googletest/archive/refs/tags/v1.15.2.tar.gz)
    FetchContent_Declare(Catch2
        GIT_REPOSITORY https://github.com/catchorg/Catch2.git
        GIT_TAG v3.7.1)
    FetchContent_MakeAvailable(googletest Catch2)

    enable_testing()
    include(GoogleTest)
    include(Catch)

    add_executable(polygon_tests tests/polygon_test.cpp)
    target_link_libraries(polygon_tests PRIVATE shapes GTest::gtest_main)
    gtest_discover_tests(polygon_tests)

    add_executable(stats_tests tests/stats_test.cpp)
    target_link_libraries(stats_tests PRIVATE shapes Catch2::Catch2WithMain)
    catch_discover_tests(stats_tests)
endif()
`;

const PRESETS = `{
  "version": 6,
  "configurePresets": [
    {
      "name": "base",
      "hidden": true,
      "generator": "Ninja",
      "binaryDir": "\${sourceDir}/build/\${presetName}",
      "cacheVariables": { "SHAPES_TESTS": "ON" }
    },
    {
      "name": "release",
      "displayName": "Release (preset)",
      "inherits": "base",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" }
    },
    {
      "name": "asan",
      "displayName": "ASan (preset)",
      "inherits": "base",
      "cacheVariables": {
        "CMAKE_BUILD_TYPE": "Debug",
        "CMAKE_CXX_FLAGS": "-fsanitize=address -fno-omit-frame-pointer"
      }
    }
  ],
  "buildPresets": [{ "name": "release", "configurePreset": "release" }],
  "testPresets": [{ "name": "release", "configurePreset": "release" }]
}
`;

const CLANG_FORMAT = `BasedOnStyle: Google
IndentWidth: 4
ColumnLimit: 100
AllowShortFunctionsOnASingleLine: Inline
`;

const POLYGON_HPP = `#pragma once

#include <cstddef>
#include <string>
#include <vector>

namespace shapes {

/// A point in the plane.
struct Point {
    double x = 0;
    double y = 0;
};

/// A simple polygon, its corners in order.
class Polygon {
public:
    explicit Polygon(std::vector<Point> corners);

    [[nodiscard]] double area() const;
    [[nodiscard]] double perimeter() const;
    [[nodiscard]] std::size_t size() const { return corners_.size(); }
    [[nodiscard]] Polygon scaled(double factor) const;
    [[nodiscard]] std::string describe() const;

private:
    std::vector<Point> corners_;
};

Polygon square(double side);

}  // namespace shapes
`;

const POLYGON_CPP = `#include "geometry/polygon.hpp"

#include <cmath>
#include <format>
#include <stdexcept>

namespace shapes {

Polygon::Polygon(std::vector<Point> corners) : corners_(std::move(corners)) {
    if (corners_.size() < 3) {
        throw std::invalid_argument("a polygon needs at least three corners");
    }
}

double Polygon::area() const {
    double twice = 0;
    for (std::size_t i = 0; i < corners_.size(); ++i) {
        const Point& a = corners_[i];
        const Point& b = corners_[(i + 1) % corners_.size()];
        twice += a.x * b.y - b.x * a.y;
    }
    return std::abs(twice) / 2;
}

double Polygon::perimeter() const {
    double sum = 0;
    for (std::size_t i = 0; i < corners_.size(); ++i) {
        const Point& a = corners_[i];
        const Point& b = corners_[(i + 1) % corners_.size()];
        sum += std::hypot(b.x - a.x, b.y - a.y);
    }
    return sum;
}

Polygon Polygon::scaled(double factor) const {
    std::vector<Point> out;
    out.reserve(corners_.size());
    for (const Point& p : corners_) {
        out.push_back({p.x * factor, p.y * factor});
    }
    return Polygon(std::move(out));
}

std::string Polygon::describe() const {
    return std::format("{} corners, area {:.2f}, perimeter {:.2f}", size(), area(), perimeter());
}

Polygon square(double side) {
    return Polygon({{0, 0}, {side, 0}, {side, side}, {0, side}});
}

}  // namespace shapes
`;

const STATS_HPP = `#pragma once

#include <vector>

namespace shapes {

double mean(const std::vector<double>& values);
double median(std::vector<double> values);

}  // namespace shapes
`;

const STATS_CPP = `#include "stats.hpp"

#include <algorithm>
#include <numeric>
#include <stdexcept>

namespace shapes {

double mean(const std::vector<double>& values) {
    if (values.empty()) throw std::invalid_argument("no values");
    return std::accumulate(values.begin(), values.end(), 0.0) / static_cast<double>(values.size());
}

double median(std::vector<double> values) {
    if (values.empty()) throw std::invalid_argument("no values");
    std::sort(values.begin(), values.end());
    const auto mid = values.size() / 2;
    return values.size() % 2 ? values[mid] : values[mid];
}

}  // namespace shapes
`;

const MAIN = `#include <iostream>
#include <vector>

#include "geometry/polygon.hpp"
#include "stats.hpp"

int main(int argc, char** argv) {
    std::vector<shapes::Polygon> polygons{
        shapes::square(2),
        shapes::Polygon({{0, 0}, {4, 0}, {0, 3}}),
        shapes::square(1.5).scaled(2),
    };

    std::vector<double> areas;
    for (const auto& p : polygons) {
        std::cout << p.describe() << '\\n';
        areas.push_back(p.area());
    }
    std::cout << "mean area: " << shapes::mean(areas) << '\\n';
    std::cout << "median area: " << shapes::median(areas) << '\\n';
    return argc > 1 && std::string(argv[1]) == "--fail" ? 1 : 0;
}
`;

const POLYGON_TEST = `#include <gtest/gtest.h>

#include "geometry/polygon.hpp"

using shapes::Polygon;
using shapes::square;

class PolygonTest : public ::testing::Test {
protected:
    Polygon unit_ = square(1);
    Polygon triangle_ = Polygon({{0, 0}, {4, 0}, {0, 3}});
};

TEST_F(PolygonTest, AreaOfSquare) {
    EXPECT_DOUBLE_EQ(square(2).area(), 4.0);
}

TEST_F(PolygonTest, PerimeterOfTriangle) {
    EXPECT_DOUBLE_EQ(triangle_.perimeter(), 12.0);
}

TEST_F(PolygonTest, ScaledKeepsCorners) {
    EXPECT_EQ(unit_.scaled(3).size(), 4u);
}

TEST(PolygonErrors, NeedsThreeCorners) {
    EXPECT_THROW(Polygon({{0, 0}, {1, 1}}), std::invalid_argument);
}

class ScaleTest : public ::testing::TestWithParam<double> {};

TEST_P(ScaleTest, AreaGrowsWithTheSquare) {
    const double f = GetParam();
    EXPECT_DOUBLE_EQ(square(1).scaled(f).area(), f * f);
}

INSTANTIATE_TEST_SUITE_P(Factors, ScaleTest, ::testing::Values(0.5, 2.0, 3.0));
`;

const STATS_TEST = `#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_floating_point.hpp>

#include "stats.hpp"

using Catch::Matchers::WithinAbs;

TEST_CASE("mean of areas", "[stats]") {
    REQUIRE_THAT(shapes::mean({4.0, 6.0, 9.0}), WithinAbs(6.333, 0.001));
}

TEST_CASE("median", "[stats]") {
    SECTION("odd count") {
        REQUIRE(shapes::median({9.0, 4.0, 6.0}) == 6.0);
    }
    SECTION("even count") {
        REQUIRE(shapes::median({1.0, 2.0, 3.0, 4.0}) == 2.5);
    }
}

TEST_CASE("empty input throws", "[stats][errors]") {
    REQUIRE_THROWS_AS(shapes::mean({}), std::invalid_argument);
}
`;

/** `?build=broken`: a perimeter with a misspelled function and an unused variable, a median that calls `std::midpoint` wrongly. */
const broken = (files: Record<string, string>): Record<string, string> => ({
  ...files,
  "src/geometry/polygon.cpp": files["src/geometry/polygon.cpp"]
    .replace("sum += std::hypot(", "sum += std::hypotenuse(")
    .replace("    std::vector<Point> out;\n", "    std::size_t count = corners_.size();\n    std::vector<Point> out;\n"),
  "src/stats.cpp": files["src/stats.cpp"].replace("values[mid] : values[mid];", "values[mid] : std::midpoint(values[mid]);"),
});

const files: Record<string, string> = {
  "CMakeLists.txt": CMAKE,
  "CMakePresets.json": PRESETS,
  "vcpkg.json": `{
  "name": "shapes",
  "version": "1.2.0",
  "dependencies": [
    "fmt",
    { "name": "gtest", "host": true },
    "catch2"
  ]
}
`,
  ".clang-format": CLANG_FORMAT,
  ".gitignore": "cmake-build-*/\nbuild/\n.cache/\n",
  "README.md": "# shapes\n\nPolygon geometry with a small command line app.\n\n```sh\ncmake --preset release\ncmake --build build/release\nctest --test-dir build/release\n```\n",
  "src/main.cpp": MAIN,
  "src/geometry/polygon.hpp": POLYGON_HPP,
  "src/geometry/polygon.cpp": POLYGON_CPP,
  "src/stats.hpp": STATS_HPP,
  "src/stats.cpp": STATS_CPP,
  "tests/polygon_test.cpp": POLYGON_TEST,
  "tests/stats_test.cpp": STATS_TEST,
};

export const cppDemoFiles: Record<string, string> = isPioDemo() ? pioDemoFiles : cppBrokenBuild() ? broken(files) : files;
