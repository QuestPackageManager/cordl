// Compile with freshly generated Unity 6.3 headers and the matching beatsaber-hook headers.
#include "UnityEngine/Quaternion.hpp"
#include "System/Int32.hpp"

#include <type_traits>

using Quaternion = UnityEngine::Quaternion;
using ReadonlyQuaternion = by_ref<Quaternion const>;

static_assert(std::is_same_v<decltype(&Quaternion::Dot), float (*)(ReadonlyQuaternion, ReadonlyQuaternion)>);
static_assert(std::is_invocable_v<decltype(&Quaternion::Dot), Quaternion const&, Quaternion const&>);
static_assert(std::is_invocable_v<decltype(&Quaternion::Dot), Quaternion, Quaternion>);
static_assert(!std::is_constructible_v<by_ref<Quaternion>, Quaternion const&>);
static_assert(!std::is_constructible_v<by_ref<Quaternion>, Quaternion&&>);

float readonly_quaternion_calls(Quaternion const& base) {
    auto normalized = Quaternion::Normalize(Quaternion{0, 0, 0, 2});
    return Quaternion::Dot(normalized, base);
}

bool mutable_out_call(StringW text, int32_t& result) {
    return System::Int32::TryParse(text, result);
}

Il2CppType const* readonly_quaternion_metadata() {
    return i2c::type_of<ReadonlyQuaternion>();
}
