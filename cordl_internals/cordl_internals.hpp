#pragma once

#include "config.hpp"
#include "method-utils.hpp"
#include "field-utils.hpp"
#include "unity-utils.hpp"

#include "beatsaber-hook/shared/byref.hpp"
#include "beatsaber-hook/shared/members.hpp"
#include "beatsaber-hook/shared/types.hpp"
#include "beatsaber-hook/shared/utils.hpp"

// TODO: Implement
template <typename T>
using ByRefConst = ::by_ref<T>;
