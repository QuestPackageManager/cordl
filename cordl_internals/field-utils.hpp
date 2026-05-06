#pragma once

#include "config.hpp"
#include "internal.hpp"
#include "exceptions.hpp"

#include "beatsaber-hook/shared/find.hpp"
#include "beatsaber-hook/shared/members.hpp"

#include <bit>
#include <cstddef>

namespace UnityEngine {
  class Object;
}

namespace {
namespace cordl_internals {

  /// @brief method to find a field info in a klass
  /// @tparam name field name
  /// @tparam klass_resolver method to get the Il2CppClass* on which to get the klass
  template<typename K>
  CORDL_HIDDEN FieldInfo* findField(std::string_view name) {
    auto klass = ::i2c::class_of<K>();
    if (!klass) throw NullException(std::string("Class for field with name: ") + name.data() + " is null!");
    auto field = ::i2c::find_field(klass, name);
    if (!field) throw FieldException(std::string("Could not find field with name: ") + name.data());
    return field;
  }

  /// @brief template for setting a static field on a class
  /// @tparam T field type
  /// @tparam name field name
  /// @tparam K the class on which the field resides
  template<typename T, ::i2c::str_lit name, typename K>
  CORDL_HIDDEN void setStaticField(T&& value) {
    static auto field = findField<K>(name.data);
    ::i2c::functions::field_static_set_value(field, const_cast<void*>(static_cast<void const*>(&value)));
  }

  /// @brief method to set a field that's an il2cpp type
  template<::i2c::type_check::full_class T, ::i2c::str_lit name, typename K>
  CORDL_HIDDEN void setStaticField(T&& value) {
    static auto field = findField<K>(name.data);
    ::i2c::functions::field_static_set_value(field, ::i2c::to_object<false, T>(value));
  }

#pragma region static field getters

  /// @brief template for getting a static field on a class
  /// @tparam T field type
  /// @tparam name field name
  /// @tparam K the class on which the field resides
  template <typename T, ::i2c::str_lit name, typename K>
  [[nodiscard]] CORDL_HIDDEN T getStaticField() {
    static auto field = findField<K>(name.data);
    T val;
    ::i2c::functions::field_static_get_value(field, static_cast<void*>(&val));
    return val;
  }

  /// @brief method to set a field that's a reference type
  template <::i2c::type_check::ref_type T, ::i2c::str_lit name, typename K>
  [[nodiscard]] CORDL_HIDDEN T getStaticField() {
    static auto field = findField<K>(name.data);
    void* val;
    ::i2c::functions::field_static_get_value(field, &val);
    return ::i2c::from_object<T, false>(val);
  }

  /// @brief method to set a field for a generic container type
  template <typename T> CORDL_HIDDEN void setInstanceField(Il2CppObject* owner, T* pointer, std::type_identity_t<T>& value) {
    // if a ref type, use wbarrier
    if constexpr (::i2c::type_check::ref_type<T>) {
      ::i2c::functions::gc_wbarrier_set_field(owner, static_cast<void**>(static_cast<void*>(pointer)), cordl_internals::convert(value));
    } else {
      // if a value type, normal assignment wokrs
      *pointer = value;
    }
  }

#pragma endregion // static field getters
}
} // end anonymous namespace
