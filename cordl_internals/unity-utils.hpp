#pragma once

#include "exceptions.hpp"
#include "internal.hpp"

#include "beatsaber-hook/shared/types.hpp"
#include "beatsaber-hook/shared/utils.hpp"

#define __UNITYW_UNITY_NULL_HANDLE_CHECK(...) \
  if (is_alive()) return __VA_ARGS__;         \
  throw ::cordl_internals::NullException(::i2c::type_name<std::remove_pointer_t<decltype(this)>>() + " is holding a null Object")

template <typename T> struct UnityW {
  constexpr UnityW() noexcept = default;
  constexpr UnityW(T* t) noexcept : val(t) {}

  template <typename U>
    requires(std::is_convertible_v<U*, T*>)
  constexpr UnityW(UnityW<U> u) : val(u.val) {}

  constexpr UnityW(nullptr_t) noexcept : val(nullptr) {}
  explicit constexpr UnityW(void* p) noexcept : val(static_cast<T*>(p)) {}

  constexpr void* convert() const noexcept {
    return const_cast<void*>(static_cast<void const*>(unsafe_ptr()));
  }

  template <typename U> inline UnityW<U> cast() const {
    auto attemptedCast = try_cast<U>();
    if (!attemptedCast) throw ::cordl_internals::CastException(::i2c::type_name<std::remove_pointer_t<decltype(this)>>() + " could not cast to " + ::i2c::type_name<UnityW<U>>());
    return attemptedCast;
  }

  template <typename U> inline UnityW<U> try_cast() const {
    auto attemptedCast = ::i2c::try_cast<U*>(const_cast<T*>(this->val));
    if (!attemptedCast) return {};
    return UnityW<U>(attemptedCast);
  }

  constexpr T* unsafe_ptr() noexcept { return val; }
  constexpr T const* unsafe_ptr() const noexcept { return val; }

  constexpr T* ptr() { __UNITYW_UNITY_NULL_HANDLE_CHECK(val); }
  constexpr T const* ptr() const { __UNITYW_UNITY_NULL_HANDLE_CHECK(val); }

  /// @brief Implicitly cast this instance to a T*.
  constexpr operator T*() noexcept { return unsafe_ptr(); }
  constexpr operator T const*() const noexcept { return unsafe_ptr(); }

  /// @brief access underlying pointer with ->
  constexpr T* operator->() { return ptr(); }
  constexpr T const* operator->() const { return ptr(); }

  constexpr operator bool() { return is_alive(); }
  constexpr operator bool() const { return is_alive(); }

  template <typename U = T>
    requires(std::is_assignable_v<T, U> || std::is_same_v<T, U>)
  constexpr bool operator==(UnityW<U> const& other) const {
    return other.is_alive() == is_alive() && other.val == val;
  }

  template <typename U = T>
    requires(std::is_assignable_v<T, U> || std::is_same_v<T, U>)
  constexpr bool operator==(U const* other) const {
    return (other && ::cordl_internals::read_cachedptr(val)) == is_alive() && other == val;
  }

  [[nodiscard]] constexpr inline bool is_alive() const {
    return val && ::cordl_internals::read_cachedptr(val);
  }

private:
  T* val;
};

MARK_GEN_REF_T(UnityW);

template <class T> struct BS_HOOK_HIDDEN ::i2c::type_check::no_arg_class<UnityW<T>> {
  static inline Il2CppClass* get() {
    // don't double cache here, just inline
    return no_arg_class<T*>::get();
  }
};
