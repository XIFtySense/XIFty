from __future__ import annotations

import ctypes
import json
import os
import sys
from dataclasses import dataclass
from enum import IntEnum
from pathlib import Path
from typing import Any


class XiftyError(RuntimeError):
    pass


class StatusCode(IntEnum):
    SUCCESS = 0
    INVALID_ARGUMENT = 1
    IO_ERROR = 2
    UNSUPPORTED_FORMAT = 3
    PARSE_ERROR = 4
    INTERNAL_ERROR = 5


class ViewMode(IntEnum):
    FULL = 0
    RAW = 1
    INTERPRETED = 2
    NORMALIZED = 3
    REPORT = 4


_VIEW_BY_NAME = {
    "full": ViewMode.FULL,
    "raw": ViewMode.RAW,
    "interpreted": ViewMode.INTERPRETED,
    "normalized": ViewMode.NORMALIZED,
    "report": ViewMode.REPORT,
}


class _Buffer(ctypes.Structure):
    _fields_ = [
        ("ptr", ctypes.POINTER(ctypes.c_uint8)),
        ("len", ctypes.c_size_t),
        ("capacity", ctypes.c_size_t),
    ]


class _Result(ctypes.Structure):
    _fields_ = [
        ("status", ctypes.c_int),
        ("output", _Buffer),
        ("error_message", _Buffer),
    ]


def _default_library_name() -> str:
    if sys.platform == "darwin":
        return "libxifty_ffi.dylib"
    if os.name == "nt":
        return "xifty_ffi.dll"
    return "libxifty_ffi.so"


def _workspace_root() -> Path:
    return Path(__file__).resolve().parents[3]


def _default_library_path() -> Path:
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", _workspace_root() / "target"))
    for profile in ("debug", "release"):
        candidate = target_dir / profile / _default_library_name()
        if candidate.exists():
            return candidate
    return target_dir / "debug" / _default_library_name()


def _buffer_to_bytes(buffer: _Buffer) -> bytes:
    if not bool(buffer.ptr) or buffer.len == 0:
        return b""
    return ctypes.string_at(buffer.ptr, buffer.len)


@dataclass
class _Binding:
    library: ctypes.CDLL

    @classmethod
    def load(cls, library_path: str | os.PathLike[str] | None = None) -> "_Binding":
        path = Path(library_path) if library_path is not None else _default_library_path()
        library = ctypes.CDLL(str(path))

        library.xifty_probe_json.argtypes = [ctypes.c_char_p]
        library.xifty_probe_json.restype = _Result

        library.xifty_extract_json.argtypes = [ctypes.c_char_p, ctypes.c_int]
        library.xifty_extract_json.restype = _Result

        library.xifty_free_buffer.argtypes = [_Buffer]
        library.xifty_free_buffer.restype = None

        library.xifty_version.argtypes = []
        library.xifty_version.restype = ctypes.c_char_p

        return cls(library=library)

    def version(self) -> str:
        return self.library.xifty_version().decode("utf-8")

    def probe(self, path: str | os.PathLike[str]) -> dict[str, Any]:
        return self._call_json(self.library.xifty_probe_json, os.fspath(path).encode("utf-8"))

    def extract(
        self,
        path: str | os.PathLike[str],
        *,
        view: str | ViewMode = ViewMode.FULL,
    ) -> dict[str, Any]:
        if isinstance(view, str):
            try:
                view_mode = _VIEW_BY_NAME[view.lower()]
            except KeyError as error:
                raise ValueError(f"unsupported view: {view}") from error
        else:
            view_mode = view
        return self._call_json(
            self.library.xifty_extract_json,
            os.fspath(path).encode("utf-8"),
            int(view_mode),
        )

    def _call_json(self, function: Any, *args: Any) -> dict[str, Any]:
        result = function(*args)
        try:
            status = StatusCode(result.status)
            output = _buffer_to_bytes(result.output)
            error = _buffer_to_bytes(result.error_message).decode("utf-8")
        finally:
            self.library.xifty_free_buffer(result.output)
            self.library.xifty_free_buffer(result.error_message)

        if status is not StatusCode.SUCCESS:
            raise XiftyError(error or f"xifty ffi call failed with status {status.name}")

        return json.loads(output.decode("utf-8"))


_BINDING = _Binding.load()


def version() -> str:
    return _BINDING.version()


def probe(path: str | os.PathLike[str]) -> dict[str, Any]:
    return _BINDING.probe(path)


def extract(
    path: str | os.PathLike[str],
    *,
    view: str | ViewMode = ViewMode.FULL,
) -> dict[str, Any]:
    return _BINDING.extract(path, view=view)
