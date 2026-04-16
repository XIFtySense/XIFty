import XCTest
@testable import XIFtySwift

final class XIFtySwiftTests: XCTestCase {
    private var root: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    func testVersionIsNonEmpty() {
        XCTAssertFalse(XIFty.version().isEmpty)
    }

    func testProbeReturnsDetectedFormat() throws {
        let output = try XIFty.probe(path: root.appendingPathComponent("fixtures/minimal/happy.jpg").path)
        let input = try XCTUnwrap(output["input"] as? [String: Any])
        XCTAssertEqual(input["detected_format"] as? String, "jpeg")
    }

    func testExtractNormalizedReturnsExpectedField() throws {
        let output = try XIFty.extract(
            path: root.appendingPathComponent("fixtures/minimal/happy.jpg").path,
            view: .normalized
        )
        let normalized = try XCTUnwrap(output["normalized"] as? [String: Any])
        let fields = try XCTUnwrap(normalized["fields"] as? [[String: Any]])
        let makeField = fields.first { ($0["field"] as? String) == "device.make" }
        let value = try XCTUnwrap(makeField?["value"] as? [String: Any])
        XCTAssertEqual(value["value"] as? String, "XIFtyCam")
    }
}
