#pragma once

#include "config.hpp"

#include <concepts>
#include <type_traits>

#include "beatsaber-hook/shared/types.hpp"

// Concepts backing the `requires` clauses cordl writes for C# generic constraints.
//
// These are built directly on beatsaber-hook's type graph (`i2c::type_check`) -- the same
// one MARK_VAL_T/MARK_REF_T/MARK_GEN_VAL_T/MARK_GEN_REF_T(_PTR) populate for cordl's
// generated types, the primitives, and the wrapper types (ArrayW, UnityW, ValueW, ...). A
// substituted generic argument has to actually be part of that graph to satisfy a
// constraint; it is never accepted just because cordl/beatsaber-hook don't recognize it.
//
// Define CORDL_NO_GENERIC_CONSTRAINTS to turn every constraint into a no-op.

namespace {
namespace cordl_internals {

#ifdef CORDL_NO_GENERIC_CONSTRAINTS

template <typename T>
concept reference_type_constraint = true;

template <typename T>
concept value_type_constraint = true;

template <typename T>
concept default_constructor_constraint = true;

template <typename T, typename U>
concept type_constraint = true;

#else

/// @brief C# `where T : class`
///
/// Covers both pointer-represented reference types (marked via MARK_REF_T/MARK_GEN_REF_T_PTR)
/// and the reference-type wrappers (StringW, ...) that hold their Il2CppObject* by value.
template <typename T>
concept reference_type_constraint = ::i2c::type_check::ref_type<T> || ::i2c::type_check::wrapper_type<T>;

/// @brief C# `where T : struct`
template <typename T>
concept value_type_constraint = ::i2c::type_check::value_type<T>;

/// @brief C# `where T : new()`
template <typename T>
concept default_constructor_constraint = std::is_default_constructible_v<std::remove_cvref_t<T>>;

/// @brief C# `where T : U`
///
/// `T` must be part of the type graph, exactly as `reference_type_constraint`/
/// `value_type_constraint` require. A base class is a real C++ base of `T`, so a pointer
/// to `T` converts to a pointer to it directly; an interface instead surfaces as a
/// conversion operator declared on the pointee, so that conversion is also tried with `T`
/// dereferenced.
template <typename T, typename U>
concept type_constraint =
    (::i2c::type_check::has_mark<T> || ::i2c::type_check::wrapper_type<T>) &&
    (std::is_convertible_v<T, U> ||
     (std::is_pointer_v<T> && std::is_convertible_v<std::remove_pointer_t<T>, U>));

#endif

} // namespace cordl_internals
} // namespace
