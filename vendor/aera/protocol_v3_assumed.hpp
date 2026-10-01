/* SPDX-License-Identifier: Apache-2.0 */
// ASSUMED: Host API 3 additions to aeraui/features/plugin_api/protocol.hpp.
//
// AERA has not published its generic pixel + GPU host. This header is our
// guess, written in AERA's own style so it can be diffed against the real
// header the day it lands (aera-flutter-demo#1). Every item is ASSUMED.
// spec/host.md, this file and src/host/ change together; tests/drift.rs
// checks the numbers.
#pragma once

#include "protocol.hpp"

namespace aeraui::plugin_api::v3 {

constexpr uint32_t kProtocolVersion = 3;     // ASSUMED
constexpr int kSurfaceFd = 3;                // ASSUMED, also AERA_SURFACE_FD
constexpr int kControlFd = 4;                // Host API 2, also AERA_PLUGIN_FD

enum Features : uint32_t {
  kFeaturePixelSurface = 1U << 2,            // ASSUMED
  kFeatureKeyboardInset = 1U << 3,           // ASSUMED
};

enum class Kind : uint32_t {
  // Plugin to host, appended after kSetBackAction (11).
  kPresent = 12,                             // ASSUMED request_id sequence, value slot
  kKeyboardShow = 13,                        // ASSUMED value purpose, flags 1 multiline
  kKeyboardHide = 14,                        // ASSUMED
  // Host to plugin, appended after kOperationResult (67).
  kSurface = 68,                             // ASSUMED value w, flags h, request_id stride,
                                             //   title "BGRA8888", text "slots=N scale=S refresh=HZ"
  kFrameDone = 69,                           // ASSUMED request_id sequence
  kTouchDown = 70,                           // ASSUMED request_id pointer, value x, flags y
  kTouchMove = 71,                           // ASSUMED
  kTouchUp = 72,                             // ASSUMED
  kKey = 73,                                 // ASSUMED value code point; 8 backspace, 13 enter/submit
  kBack = 74,                                // ASSUMED
  kKeyboardInset = 75,                       // ASSUMED value height in surface pixels, 0 hidden
};

enum class Purpose : uint32_t {             // ASSUMED, as AERA Browser's keyboard
  kText = 0,                                 // ASSUMED
  kDigits = 2,                               // ASSUMED
};

}  // namespace aeraui::plugin_api::v3
