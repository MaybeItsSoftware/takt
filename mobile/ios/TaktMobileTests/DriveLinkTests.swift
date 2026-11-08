import Foundation
import XCTest

@testable import Takt

/// Google's Picker answers on iOS (docs/links.md "Phones").
final class DriveLinkTests: XCTestCase {
  func testThePickersRedirectNamesTheFiles() {
    let items = URLComponents(string: "x:/oauth2redirect?code=c&picked_file_ids=1a,2b,%203c&state=s")!.queryItems!
    XCTAssertEqual(GoogleCalendarAuth.pickedIDs(items), ["1a", "2b", "3c"])
    XCTAssertEqual(GoogleCalendarAuth.pickedIDs([]), [])
  }

  func testAPickLinksByDrivesOwnAddress() {
    let sheet = GoogleDriveFiles.read(
      id: "1",
      json: Data(
        #"{"name":"Budget","mimeType":"application/vnd.google-apps.spreadsheet","webViewLink":"https://docs.google.com/spreadsheets/d/1/edit"}"#
          .utf8))
    XCTAssertEqual(
      sheet,
      PickedDriveFile(
        url: "https://docs.google.com/spreadsheets/d/1/edit", name: "Budget",
        mimeType: "application/vnd.google-apps.spreadsheet"))
    let folder = GoogleDriveFiles.read(
      id: "2", json: Data(#"{"name":"Trips","mimeType":"application/vnd.google-apps.folder"}"#.utf8))
    XCTAssertEqual(folder?.url, "https://drive.google.com/drive/folders/2")
    let pdf = GoogleDriveFiles.read(id: "3", json: Data(#"{"name":"","webViewLink":"javascript:x"}"#.utf8))
    XCTAssertEqual(pdf?.url, "https://drive.google.com/file/d/3/view")
    XCTAssertEqual(pdf?.name, "Untitled")
    XCTAssertNil(GoogleDriveFiles.read(id: "../x", json: Data("{}".utf8)))
    XCTAssertFalse(GoogleDriveFiles.isID("é1"))
  }
}
