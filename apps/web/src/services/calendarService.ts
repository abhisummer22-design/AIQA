export interface CalendarEventItem {
  id: string;
  summary: string;
  description?: string;
  location?: string;
  start: {
    dateTime?: string;
    date?: string;
    timeZone?: string;
  };
  end: {
    dateTime?: string;
    date?: string;
    timeZone?: string;
  };
  status?: string;
  htmlLink?: string;
  attendees?: Array<{ email: string; responseStatus?: string; displayName?: string }>;
  creator?: { email: string; displayName?: string };
}

export interface CreateEventPayload {
  summary: string;
  description?: string;
  location?: string;
  startDateTime: string; // ISO 8601 string
  endDateTime: string;   // ISO 8601 string
  timeZone?: string;
  attendees?: string[]; // array of emails
}

export const listCalendarEvents = async (
  token: string,
  options?: {
    timeMin?: string;
    timeMax?: string;
    maxResults?: number;
    calendarId?: string;
  }
): Promise<CalendarEventItem[]> => {
  const calId = encodeURIComponent(options?.calendarId || "primary");
  const url = new URL(`https://www.googleapis.com/calendar/v3/calendars/${calId}/events`);
  
  // Default to from start of today or recent days if not provided
  const timeMin = options?.timeMin || new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString();
  url.searchParams.set("timeMin", timeMin);
  if (options?.timeMax) {
    url.searchParams.set("timeMax", options.timeMax);
  }
  url.searchParams.set("maxResults", (options?.maxResults || 25).toString());
  url.searchParams.set("singleEvents", "true");
  url.searchParams.set("orderBy", "startTime");

  const res = await fetch(url.toString(), {
    headers: { Authorization: `Bearer ${token}` }
  });

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to list calendar events: ${err}`);
  }

  const data = await res.json();
  return (data.items || []).map((item: any) => ({
    id: item.id,
    summary: item.summary || "(No Title)",
    description: item.description,
    location: item.location,
    start: item.start || {},
    end: item.end || {},
    status: item.status,
    htmlLink: item.htmlLink,
    attendees: item.attendees || [],
    creator: item.creator
  }));
};

export const createCalendarEvent = async (
  token: string,
  payload: CreateEventPayload,
  calendarId: string = "primary"
): Promise<CalendarEventItem> => {
  const calId = encodeURIComponent(calendarId);
  const timeZone = payload.timeZone || Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";

  const body = {
    summary: payload.summary,
    description: payload.description,
    location: payload.location || "AIQA Testing Cloud Pipeline",
    start: {
      dateTime: payload.startDateTime,
      timeZone
    },
    end: {
      dateTime: payload.endDateTime,
      timeZone
    },
    attendees: (payload.attendees || []).map((email) => ({ email })),
    reminders: {
      useDefault: false,
      overrides: [
        { method: "popup", minutes: 15 },
        { method: "email", minutes: 30 }
      ]
    }
  };

  const res = await fetch(`https://www.googleapis.com/calendar/v3/calendars/${calId}/events`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify(body)
  });

  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Failed to create calendar event: ${err}`);
  }

  return res.json();
};

export const deleteCalendarEvent = async (
  token: string,
  eventId: string,
  calendarId: string = "primary"
): Promise<void> => {
  const calId = encodeURIComponent(calendarId);
  const res = await fetch(`https://www.googleapis.com/calendar/v3/calendars/${calId}/events/${eventId}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${token}` }
  });

  if (!res.ok && res.status !== 204 && res.status !== 410) {
    const err = await res.text();
    throw new Error(`Failed to delete calendar event: ${err}`);
  }
};
