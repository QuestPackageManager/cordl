#pragma once

#include "beatsaber-hook/shared/exceptions.hpp"

namespace {
namespace cordl_internals {
struct FieldException : public ::i2c::trace_exception {
  using trace_exception::trace_exception;
};

struct NullException : public ::i2c::trace_exception {
  using trace_exception::trace_exception;
};

struct CastException : public ::i2c::trace_exception {
  using trace_exception::trace_exception;
};
} // namespace cordl_internals
} // end anonymous namespace
