#include <napi.h>

extern "C" {
#include "xifty.h"
}

#include <string>

namespace {

std::string BufferToString(const XiftyBuffer& buffer) {
  if (buffer.ptr == nullptr || buffer.len == 0) {
    return std::string();
  }
  return std::string(reinterpret_cast<const char*>(buffer.ptr), buffer.len);
}

void FreeResult(XiftyResult result) {
  xifty_free_buffer(result.output);
  xifty_free_buffer(result.error_message);
}

void ThrowFromResult(Napi::Env env, const XiftyResult& result) {
  const std::string message = BufferToString(result.error_message);
  std::string prefix = "xifty ffi error";
  switch (result.status) {
    case XIFTY_STATUS_CODE_INVALID_ARGUMENT:
      prefix = "xifty invalid argument";
      break;
    case XIFTY_STATUS_CODE_IO_ERROR:
      prefix = "xifty io error";
      break;
    case XIFTY_STATUS_CODE_UNSUPPORTED_FORMAT:
      prefix = "xifty unsupported format";
      break;
    case XIFTY_STATUS_CODE_PARSE_ERROR:
      prefix = "xifty parse error";
      break;
    case XIFTY_STATUS_CODE_INTERNAL_ERROR:
      prefix = "xifty internal error";
      break;
    default:
      break;
  }

  if (message.empty()) {
    Napi::Error::New(env, prefix).ThrowAsJavaScriptException();
  } else {
    Napi::Error::New(env, prefix + ": " + message).ThrowAsJavaScriptException();
  }
}

Napi::Value Version(const Napi::CallbackInfo& info) {
  const Napi::Env env = info.Env();
  return Napi::String::New(env, xifty_version());
}

Napi::Value ProbeJson(const Napi::CallbackInfo& info) {
  const Napi::Env env = info.Env();
  if (info.Length() != 1 || !info[0].IsString()) {
    Napi::TypeError::New(env, "probeJson expects a file path string")
        .ThrowAsJavaScriptException();
    return env.Null();
  }

  const std::string path = info[0].As<Napi::String>().Utf8Value();
  const XiftyResult result = xifty_probe_json(path.c_str());

  if (result.status != XIFTY_STATUS_CODE_SUCCESS) {
    ThrowFromResult(env, result);
    FreeResult(result);
    return env.Null();
  }

  const std::string json = BufferToString(result.output);
  FreeResult(result);
  return Napi::String::New(env, json);
}

Napi::Value ExtractJson(const Napi::CallbackInfo& info) {
  const Napi::Env env = info.Env();
  if (info.Length() != 2 || !info[0].IsString() || !info[1].IsNumber()) {
    Napi::TypeError::New(env, "extractJson expects a file path string and numeric view mode")
        .ThrowAsJavaScriptException();
    return env.Null();
  }

  const std::string path = info[0].As<Napi::String>().Utf8Value();
  const int32_t view_mode = info[1].As<Napi::Number>().Int32Value();
  const XiftyResult result =
      xifty_extract_json(path.c_str(), static_cast<XiftyViewMode>(view_mode));

  if (result.status != XIFTY_STATUS_CODE_SUCCESS) {
    ThrowFromResult(env, result);
    FreeResult(result);
    return env.Null();
  }

  const std::string json = BufferToString(result.output);
  FreeResult(result);
  return Napi::String::New(env, json);
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set("version", Napi::Function::New(env, Version));
  exports.Set("probeJson", Napi::Function::New(env, ProbeJson));
  exports.Set("extractJson", Napi::Function::New(env, ExtractJson));
  return exports;
}

}  // namespace

NODE_API_MODULE(xifty_node, Init)
