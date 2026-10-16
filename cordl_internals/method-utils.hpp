#pragma once

#include "config.hpp"
#include "internal.hpp"
#include "exceptions.hpp"

#include "beatsaber-hook/shared/members.hpp"
#include "il2cpp-tabledefs.h"

#include <type_traits>
#include <sstream>
#include <array>
#include <bit>
#include <cstddef>

namespace UnityEngine {
class Object;
}

namespace {
namespace cordl_internals {

#if defined(UNITY_6) && !defined(CORDL_NO_SHARED_GENERIC_INVOKE)
struct SharedGenericInvocation {
  MethodInfo const* method;
  void* object;
  void* result;
  std::size_t value_size;
};

// The invoker adapts fully shared signatures, including variable-sized values.
// Keep the original MethodInfo: Unity's redirect also uses private metadata
// following it. Forward methodPointer even when null; substituting the virtual
// entry can select an adjustor thunk that expects a boxed receiver.
CORDL_HIDDEN inline void InvokeSharedGeneric(Il2CppMethodPointer, MethodInfo const*, void* object, void** arguments, void*) {
  auto& call = *static_cast<SharedGenericInvocation*>(object);
  if (call.value_size) {
    auto* klass = ::i2c::functions::class_from_il2cpp_type(call.method->return_type);
    if (static_cast<std::size_t>(::i2c::functions::class_value_size(klass, nullptr)) != call.value_size) {
      ::i2c::result_or_throw<void>("Return type size does not match");
    }
  }
  call.method->invoker_method(call.method->methodPointer, call.method, call.object, arguments, call.result);
}

template <typename TOut, typename T, typename... TArgs> CORDL_HIDDEN TOut RunSharedGenericValueMethod(T&& instance, MethodInfo const* method, TArgs&&... params) {
  ::i2c::functions::initialize();
  if (!method->invoker_method || (!method->methodPointer && !method->virtualMethodPointer)) {
    return ::i2c::result_or_throw<TOut>("Method pointer or shared invoker cannot be null");
  }
  // As in run_method(MethodInfo*), lookup has already checked the signature.
  void* object = method->flags & METHOD_ATTRIBUTE_STATIC ? nullptr : ::i2c::to_object<false>(instance);
  std::array<void*, sizeof...(TArgs)> arguments{ ::i2c::to_object<false>(params)... };
  auto invoke = [&](void* result, std::size_t value_size = 0) {
    SharedGenericInvocation call{ method, object, result, value_size };
    // Retain IL2CPP's exception boundary and static class initialization. A void
    // trampoline lets the real invoker write to our result buffer without
    // runtime_invoke boxing its return. A mod-side catch of the runtime's C++
    // exception wrapper is not portable across the two shared libraries.
    MethodInfo boundary{};
    boundary.name = method->name;
    boundary.klass = method->klass;
    boundary.flags = method->flags & METHOD_ATTRIBUTE_STATIC;
    boundary.return_type = ::i2c::type_of<void>();
    boundary.invoker_method = &InvokeSharedGeneric;
    Il2CppException* exception = nullptr;
    ::i2c::functions::runtime_invoke(&boundary, &call, arguments.data(), &exception);
    if (exception) {
      ::i2c::result_or_throw<void>(fmt::format("Method: {} failed with an exception: {}", method->name, ::i2c::exception_to_string(exception)));
    }
  };
  alignas(TOut) std::array<std::byte, sizeof(TOut)> result{};
  invoke(result.data(), sizeof(TOut));
  return std::bit_cast<TOut>(result);
}

// Modified version of beatsaber-hook's i2c::detail::run_method_impl that uses
// already-resolved metadata for void/reference returns and accepts a null
// methodPointer when a valid shared invoker and virtual entry exist.
// https://github.com/QuestPackageManager/beatsaber-hook/blob/939d1c2850ca963c87e928f80186cfc8c32d25de/shared/members.hpp#L14-L60
template <typename TOut, typename T, typename... TArgs> CORDL_HIDDEN TOut RunSharedGenericRuntimeMethod(T&& instance, MethodInfo const* method, TArgs&&... params) {
  ::i2c::functions::initialize();
  if (!method->invoker_method || (!method->methodPointer && !method->virtualMethodPointer)) {
    return ::i2c::result_or_throw<TOut>("Method pointer or shared invoker cannot be null");
  }
  void* object = method->flags & METHOD_ATTRIBUTE_STATIC ? nullptr : ::i2c::to_object<false>(instance);
  std::array<void*, sizeof...(TArgs)> arguments{ ::i2c::to_object<false>(params)... };
  Il2CppException* exception = nullptr;
  [[maybe_unused]] auto* result = ::i2c::functions::runtime_invoke(method, object, arguments.data(), &exception);
  if (exception) {
    ::i2c::result_or_throw<void>(fmt::format("Method: {} failed with an exception: {}", method->name, ::i2c::exception_to_string(exception)));
  }
  if constexpr (!std::is_void_v<TOut>) {
    return ::i2c::from_object<TOut, true>(result);
  }
}
#endif

template <typename TOut = void, typename T, typename... TArgs> CORDL_HIDDEN TOut RunMethodRethrow(T&& instance, MethodInfo const* method, TArgs&&... params) {
  CRASH_UNLESS(method);

  // do a null check for reference instance method calls
#ifndef NO_RUNTIME_INSTANCE_METHOD_NULL_CHECKS
  if constexpr (::i2c::type_check::ref_type<T>) {
    // get the instance value, regardless of if it is boxed or anything
    auto inst = ::i2c::to_object<false>(instance);

    if ((method->flags & METHOD_ATTRIBUTE_STATIC) == 0) { // method is instance method
      if (!inst) {
        // if inst evaluates false, we are dealing with a nullptr instance, and the instance method call is a bad idea
        std::stringstream str;
        // FIXME: should we use this string, or something else? log a stacktrace?
        str << "Instance was null for method call of ";
        str << method->klass->name;
        str << "::";
        str << method->name;
        throw NullException(str.str());
      }

#ifndef ALLOW_INVALID_UNITY_METHOD_CALLS
      if constexpr (std::is_convertible_v<T, UnityEngine::Object*>) {
        if (!::cordl_internals::read_cachedptr(static_cast<UnityEngine::Object*>(inst))) {
          // if cached ptr evaluates as false, we are dealing with an invalid unity instance, and the instance method call is a bad idea
          std::stringstream str;
          // FIXME: should we use this string, or something else? log a stacktrace?
          str << "Instance was null for method call of ";
          str << method->klass->name;
          str << "::";
          str << method->name;
          throw NullException(str.str());
        }
      }
#endif
    }
  }
#endif

  //   if constexpr (checkTypes && sizeof...(TArgs) > 0) { // param type check
  //     std::array<Il2CppType const*, sizeof...(TArgs)> types{ ExtractType(params)... };
  //     // TODO: check types array against types in methodinfo

  //     auto outType = ExtractType<TOut>();
  //     if (outType) {
  //       // TODO: check return type against methodinfo return type
  //     }
  //   }

#if defined(UNITY_6) && !defined(CORDL_NO_SHARED_GENERIC_INVOKE)
  // Wrapper value types may have a native representation different from their
  // managed payload. Leave those, byref returns and open methods on the normal
  // runtime path. Generated C++ value types are trivially copyable payloads.
  if constexpr (::i2c::type_check::value_type<TOut> && std::is_trivially_copyable_v<TOut> && !::i2c::type_check::wrapper_type<TOut>) {
    if (method->has_full_generic_sharing_signature && !method->is_generic && !method->return_type->byref && method->return_type->valuetype) {
      return RunSharedGenericValueMethod<TOut>(std::forward<T>(instance), method, std::forward<TArgs>(params)...);
    }
  } else if constexpr (std::is_void_v<TOut> || ::i2c::type_check::ref_type<TOut>) {
    // No return-buffer trampoline is needed. Runtime invocation supports a
    // missing direct entry when the original shared invoker/virtual entry exist.
    if (method->has_full_generic_sharing_signature && !method->is_generic) {
      return RunSharedGenericRuntimeMethod<TOut>(std::forward<T>(instance), method, std::forward<TArgs>(params)...);
    }
  }
#endif
  return ::i2c::run_method<TOut>(std::forward<T>(instance), method, std::forward<TArgs>(params)...);
}
} // namespace cordl_internals
} // end anonymous namespace
