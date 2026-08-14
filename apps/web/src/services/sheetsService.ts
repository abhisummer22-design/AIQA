export interface SpreadsheetFile {
  id: string;
  name: string;
  modifiedTime: string;
  webViewLink?: string;
  owners?: Array<{ displayName: string; emailAddress: string }>;
}

export interface SheetMetadata {
  spreadsheetId: string;
  title: string;
  sheets: Array<{
    sheetId: number;
    title: string;
    index: number;
    rowCount?: number;
    columnCount?: number;
  }>;
  spreadsheetUrl: string;
}

export interface SheetValuesResponse {
  range: string;
  majorDimension: string;
  values: string[][];
}

// List user spreadsheets via Drive API
export const listSpreadsheets = async (token: string, pageSize: number = 20): Promise<SpreadsheetFile[]> => {
  const query = "mimeType='application/vnd.google-apps.spreadsheet' and trashed=false";
  const url = new URL("https://www.googleapis.com/drive/v3/files");
  url.searchParams.set("q", query);
  url.searchParams.set("pageSize", pageSize.toString());
  url.searchParams.set("fields", "files(id, name, modifiedTime, webViewLink, owners)");
  url.searchParams.set("orderBy", "modifiedTime desc");

  const res = await fetch(url.toString(), {
    headers: { Authorization: `Bearer ${token}` }
  });

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to list spreadsheets: ${err}`);
  }

  const data = await res.json();
  return data.files || [];
};

// Get spreadsheet structure and tabs
export const getSpreadsheetMetadata = async (token: string, spreadsheetId: string): Promise<SheetMetadata> => {
  const res = await fetch(`https://sheets.googleapis.com/v4/spreadsheets/${spreadsheetId}`, {
    headers: { Authorization: `Bearer ${token}` }
  });

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to get spreadsheet metadata: ${err}`);
  }

  const data = await res.json();
  return {
    spreadsheetId: data.spreadsheetId,
    title: data.properties?.title || "Untitled Spreadsheet",
    spreadsheetUrl: data.spreadsheetUrl || `https://docs.google.com/spreadsheets/d/${spreadsheetId}/edit`,
    sheets: (data.sheets || []).map((s: any) => ({
      sheetId: s.properties?.sheetId,
      title: s.properties?.title || "Sheet1",
      index: s.properties?.index || 0,
      rowCount: s.properties?.gridProperties?.rowCount,
      columnCount: s.properties?.gridProperties?.columnCount
    }))
  };
};

// Read range values
export const getSheetRangeValues = async (
  token: string,
  spreadsheetId: string,
  range: string = "A1:Z100"
): Promise<SheetValuesResponse> => {
  const res = await fetch(
    `https://sheets.googleapis.com/v4/spreadsheets/${spreadsheetId}/values/${encodeURIComponent(range)}`,
    {
      headers: { Authorization: `Bearer ${token}` }
    }
  );

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to fetch sheet values: ${err}`);
  }

  const data = await res.json();
  return {
    range: data.range || range,
    majorDimension: data.majorDimension || "ROWS",
    values: data.values || []
  };
};

// Create a new Google Spreadsheet
export const createSpreadsheet = async (
  token: string,
  title: string,
  initialSheets?: Array<{ title: string; rows: (string | number)[][] }>
): Promise<SheetMetadata> => {
  const sheetsPayload = (initialSheets || [{ title: "QA Test Matrix", rows: [] }]).map((sheet) => ({
    properties: { title: sheet.title },
    data: sheet.rows.length > 0 ? [
      {
        startRow: 0,
        startColumn: 0,
        rowData: sheet.rows.map((row) => ({
          values: row.map((val) => ({
            userEnteredValue: typeof val === "number" ? { numberValue: val } : { stringValue: String(val) }
          }))
        }))
      }
    ] : []
  }));

  const res = await fetch("https://sheets.googleapis.com/v4/spreadsheets", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      properties: { title },
      sheets: sheetsPayload
    })
  });

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to create spreadsheet: ${err}`);
  }

  const data = await res.json();
  return {
    spreadsheetId: data.spreadsheetId,
    title: data.properties?.title || title,
    spreadsheetUrl: data.spreadsheetUrl || `https://docs.google.com/spreadsheets/d/${data.spreadsheetId}/edit`,
    sheets: (data.sheets || []).map((s: any) => ({
      sheetId: s.properties?.sheetId,
      title: s.properties?.title || "Sheet1",
      index: s.properties?.index || 0
    }))
  };
};

// Append rows to an existing spreadsheet
export const appendSheetRows = async (
  token: string,
  spreadsheetId: string,
  range: string,
  values: (string | number)[][]
): Promise<{ updatedRows: number; updatedRange: string }> => {
  const url = `https://sheets.googleapis.com/v4/spreadsheets/${spreadsheetId}/values/${encodeURIComponent(
    range
  )}:append?valueInputOption=USER_ENTERED&insertDataOption=INSERT_ROWS`;

  const res = await fetch(url, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      values
    })
  });

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to append rows to sheet: ${err}`);
  }

  const data = await res.json();
  return {
    updatedRows: data.updates?.updatedRows || values.length,
    updatedRange: data.updates?.updatedRange || range
  };
};
