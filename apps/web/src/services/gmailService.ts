export interface GmailMessageSummary {
  id: string;
  threadId: string;
  subject: string;
  from: string;
  to: string;
  date: string;
  snippet: string;
  labels: string[];
}

export interface GmailProfile {
  emailAddress: string;
  messagesTotal: number;
  threadsTotal: number;
  historyId: string;
}

export interface SendEmailPayload {
  to: string;
  subject: string;
  body: string;
  htmlBody?: string;
  cc?: string;
}

// Encode email into web-safe base64 RFC 2822 format
function buildRawEmail(payload: SendEmailPayload): string {
  const { to, subject, body, htmlBody, cc } = payload;
  const emailLines = [
    `To: ${to}`,
    ...(cc ? [`Cc: ${cc}`] : []),
    `Subject: ${subject}`,
    "Content-Type: text/html; charset=utf-8",
    "MIME-Version: 1.0",
    "",
    htmlBody || body.replace(/\n/g, "<br/>")
  ];

  const utf8String = unescape(encodeURIComponent(emailLines.join("\r\n")));
  return btoa(utf8String)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

export const getGmailProfile = async (token: string): Promise<GmailProfile> => {
  const res = await fetch("https://gmail.googleapis.com/gmail/v1/users/me/profile", {
    headers: { Authorization: `Bearer ${token}` }
  });
  if (!res.ok) {
    const errorText = await res.text();
    throw new Error(`Gmail API error (${res.status}): ${errorText}`);
  }
  return res.json();
};

export const listGmailMessages = async (
  token: string,
  query: string = "",
  maxResults: number = 15
): Promise<GmailMessageSummary[]> => {
  const url = new URL("https://gmail.googleapis.com/gmail/v1/users/me/messages");
  url.searchParams.set("maxResults", maxResults.toString());
  if (query) {
    url.searchParams.set("q", query);
  }

  const res = await fetch(url.toString(), {
    headers: { Authorization: `Bearer ${token}` }
  });

  if (!res.ok) {
    const errorText = await res.text();
    throw new Error(`Failed to list Gmail messages: ${errorText}`);
  }

  const data = await res.json();
  if (!data.messages || data.messages.length === 0) {
    return [];
  }

  // Fetch header summaries for messages in parallel
  const details = await Promise.all(
    data.messages.slice(0, maxResults).map(async (msg: { id: string; threadId: string }) => {
      try {
        const itemRes = await fetch(
          `https://gmail.googleapis.com/gmail/v1/users/me/messages/${msg.id}?format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Date`,
          {
            headers: { Authorization: `Bearer ${token}` }
          }
        );
        if (!itemRes.ok) return null;
        const itemData = await itemRes.json();
        const headers = itemData.payload?.headers || [];
        const getHeader = (name: string) =>
          headers.find((h: { name: string; value: string }) => h.name.toLowerCase() === name.toLowerCase())?.value || "";

        return {
          id: itemData.id,
          threadId: itemData.threadId,
          subject: getHeader("Subject") || "(No Subject)",
          from: getHeader("From") || "Unknown Sender",
          to: getHeader("To") || "Me",
          date: getHeader("Date") || new Date().toISOString(),
          snippet: itemData.snippet || "",
          labels: itemData.labelIds || []
        } as GmailMessageSummary;
      } catch (err) {
        console.warn("Failed to fetch message details:", err);
        return null;
      }
    })
  );

  return details.filter((m): m is GmailMessageSummary => m !== null);
};

export const sendGmailMessage = async (
  token: string,
  payload: SendEmailPayload
): Promise<{ id: string; threadId: string }> => {
  const raw = buildRawEmail(payload);

  const res = await fetch("https://gmail.googleapis.com/gmail/v1/users/me/messages/send", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({ raw })
  });

  if (!res.ok) {
    const errorText = await res.text();
    throw new Error(`Failed to send email via Gmail: ${errorText}`);
  }

  return res.json();
};

export const createGmailDraft = async (
  token: string,
  payload: SendEmailPayload
): Promise<{ id: string; message: { id: string } }> => {
  const raw = buildRawEmail(payload);

  const res = await fetch("https://gmail.googleapis.com/gmail/v1/users/me/drafts", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      message: { raw }
    })
  });

  if (!res.ok) {
    const errorText = await res.text();
    throw new Error(`Failed to create Gmail draft: ${errorText}`);
  }

  return res.json();
};

export const deleteGmailMessage = async (token: string, messageId: string): Promise<void> => {
  const res = await fetch(`https://gmail.googleapis.com/gmail/v1/users/me/messages/${messageId}/trash`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` }
  });

  if (!res.ok) {
    const errorText = await res.text();
    throw new Error(`Failed to move message to trash: ${errorText}`);
  }
};
