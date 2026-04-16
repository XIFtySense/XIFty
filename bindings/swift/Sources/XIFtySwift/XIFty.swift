import CXIFty
import Foundation

public enum XIFtyViewMode: Int32 {
    case full = 0
    case raw = 1
    case interpreted = 2
    case normalized = 3
    case report = 4
}

public enum XIFtySwiftError: Error, CustomStringConvertible {
    case ffi(status: Int32, message: String)
    case invalidUTF8
    case invalidJSON

    public var description: String {
        switch self {
        case let .ffi(status, message):
            return "xifty ffi error \(status): \(message)"
        case .invalidUTF8:
            return "xifty returned invalid UTF-8"
        case .invalidJSON:
            return "xifty returned invalid JSON"
        }
    }
}

public enum XIFty {
    public static func version() -> String {
        String(cString: xifty_version())
    }

    public static func probe(path: String) throws -> [String: Any] {
        try path.withCString { cPath in
            try decode(result: xifty_probe_json(cPath))
        }
    }

    public static func extract(path: String, view: XIFtyViewMode = .full) throws -> [String: Any] {
        try path.withCString { cPath in
            try decode(
                result: xifty_extract_json(
                    cPath,
                    CXIFty.XiftyViewMode(rawValue: UInt32(view.rawValue))
                )
            )
        }
    }

    private static func decode(result: XiftyResult) throws -> [String: Any] {
        defer {
            xifty_free_buffer(result.output)
            xifty_free_buffer(result.error_message)
        }

        if result.status != XIFTY_STATUS_CODE_SUCCESS {
            let message = String(bytes: bufferBytes(result.error_message), encoding: .utf8) ?? ""
            throw XIFtySwiftError.ffi(status: Int32(result.status.rawValue), message: message)
        }

        guard let string = String(bytes: bufferBytes(result.output), encoding: .utf8) else {
            throw XIFtySwiftError.invalidUTF8
        }
        guard let data = string.data(using: .utf8),
              let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            throw XIFtySwiftError.invalidJSON
        }
        return json
    }

    private static func bufferBytes(_ buffer: XiftyBuffer) -> [UInt8] {
        guard let ptr = buffer.ptr, buffer.len > 0 else {
            return []
        }
        return Array(UnsafeBufferPointer(start: ptr, count: Int(buffer.len)))
    }
}
