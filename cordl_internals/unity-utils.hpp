#pragma once

#include "beatsaber-hook/shared/utils/typedefs.h"
#include "concepts.hpp"
#include "exceptions.hpp"

#if __has_feature(cxx_exceptions)
#define __UNITYW_UNITY_NULL_HANDLE_CHECK(...)                                  \
  if (isAlive())                                                               \
    return __VA_ARGS__;                                                        \
  throw ::cordl_internals::NullException(std::string(typeid(*this).name()) +   \
                                         " is holding a null Object")

#else
#include "beatsaber-hook/shared/utils/utils.h"
#define __UNITYW_UNITY_NULL_HANDLE_CHECK(...)                                  \
  if (isAlive())                                                               \
    return __VA_ARGS__;                                                        \
  SAFE_ABORT_MSG(std::string(typeid(*this).name()) + "Unity object with null handle was accessed!")
#endif

template <typename T> struct UnityW {
  constexpr UnityW() noexcept = default;
  constexpr UnityW(T *t) noexcept : innerPtr(t) {}

  template <typename U>
    requires(std::is_convertible_v<U *, T *>)
  constexpr UnityW(UnityW<U> u) : innerPtr(u.innerPtr) {}

  constexpr UnityW(nullptr_t) noexcept : innerPtr(nullptr) {}
  explicit constexpr UnityW(void *p) noexcept : innerPtr(static_cast<T *>(p)) {}

  constexpr void *convert() const noexcept {
    return const_cast<void *>(static_cast<void const *>(unsafePtr()));
  }

  template <typename U> inline UnityW<U> cast() const {
    auto attemptedCast = try_cast<U>();
    if (!attemptedCast) throw ::cordl_internals::CastException(std::string(typeid(*this).name()) + " could not cast to " + std::string(typeid(UnityW<U>).name()));
    return UnityW<U>(attemptedCast.value());
  }

  template <typename U> inline std::optional<UnityW<U>> try_cast() const {
    auto attemptedCast = ::il2cpp_utils::try_cast<U>(const_cast<T *>(this->innerPtr));
    if (!attemptedCast)
      return std::nullopt;

    return UnityW<U>(attemptedCast.value());
  }

  constexpr T const *unsafePtr() const noexcept { return innerPtr; }

  constexpr T *unsafePtr() noexcept { return innerPtr; }

  constexpr T *ptr() { __UNITYW_UNITY_NULL_HANDLE_CHECK(innerPtr); }

  constexpr T const *ptr() const { __UNITYW_UNITY_NULL_HANDLE_CHECK(innerPtr); }

  /// @brief Implicitly cast this instance to a T*.
  constexpr operator T *() const noexcept { return const_cast<T *>(unsafePtr()); }

  /// @brief access underlying pointer with ->
  constexpr T *operator->() { return const_cast<T *>(ptr()); }

  /// @brief access underlying pointer with ->
  constexpr T const *operator->() const { return ptr(); }

  /// @brief get access to the underlying object as a reference
  constexpr T &operator*() { return *ptr(); }

  /// @brief get access to the underlying object as a reference
  constexpr T const &operator*() const { return *ptr(); }

  constexpr operator bool() const { return isAlive(); }

  template <typename U = T>
    requires(std::is_assignable_v<T, U> || std::is_same_v<T, U>)
  constexpr bool operator==(UnityW<U> const &other) const {
    return other.isAlive() == isAlive() && other.innerPtr == innerPtr;
  }

  template <typename U = T> constexpr bool operator==(U const *other) const {
    return isAlive(other) == isAlive() && other == innerPtr;
  }

  [[nodiscard]] constexpr inline bool isAlive() const {
    return isAlive(innerPtr);
  }

  [[nodiscard]] static constexpr inline bool isAlive(T const *ptr) {
    return ptr && cached_ptr(ptr);
  }

  [[nodiscard]] static constexpr inline void *cached_ptr(T const *ptr) {
    return ptr->m_CachedPtr;
  }


private:
  T *innerPtr;
};

MARK_GEN_REF_T(UnityW);

// static_assert(il2cpp_utils::has_il2cpp_conversion<UnityW<Il2CppObject>>);

template <class T>
struct BS_HOOKS_HIDDEN ::il2cpp_utils::il2cpp_type_check::need_box<UnityW<T>> {
  constexpr static bool value = false;
};

template <class T>
struct ::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_type<UnityW<T>> {
  static inline const Il2CppType* get() {
    // don't double cache here, just inline
    return ::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_type<T *>::get();
  }
};

// technically not a class
// but we do this to allow generics to work as intended
template <class T>
struct ::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_class<UnityW<T>> {
  static inline Il2CppClass *get() {
    // don't double cache here, just inline
    return ::il2cpp_utils::il2cpp_type_check::il2cpp_no_arg_class<T *>::get();
  }
};
