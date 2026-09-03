// Tiny, dependency-free HTTP server (Node's built-in `http` only) used
// ONLY by tests/crawler.spec.ts's own real, self-contained verification —
// this is NOT part of the crawler mechanism itself, which points at
// whatever real base_url a caller configures. Simulates a minimal RBAC'd
// app: a login page, a session cookie, and role-gated pages, so a real
// crawl using different real role credentials produces a genuinely
// different, real per-role visibility matrix — not a scripted fixture
// standing in for the diff itself.
//
// Security note: plaintext in-memory credentials, no CSRF/hashing/rate
// limiting — deliberately, this is a disposable test fixture, never
// started outside this one test file, never exposed beyond localhost.

const http = require("http");
const { URL } = require("url");

const USERS = {
  admin: { password: "admin-pass", role: "admin" },
  alice: { password: "alice-pass", role: "user" },
};

function parseCookies(req) {
  const header = req.headers.cookie || "";
  const cookies = {};
  header.split(";").forEach((part) => {
    const eq = part.indexOf("=");
    if (eq > -1) {
      cookies[part.slice(0, eq).trim()] = part.slice(eq + 1).trim();
    }
  });
  return cookies;
}

function page(title, bodyHtml) {
  return `<!doctype html><html><head><title>${title}</title></head><body>${bodyHtml}</body></html>`;
}

function readBody(req) {
  return new Promise((resolve) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => resolve(body));
  });
}

function nav(role) {
  const links = ['<a href="/dashboard">Dashboard</a>', '<a href="/profile">Profile</a>'];
  if (role === "admin") {
    links.push('<a href="/settings">Settings</a>', '<a href="/users">Manage Users</a>');
  }
  links.push('<a href="/logout">Log out</a>');
  return `<nav>${links.join(" | ")}</nav>`;
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const cookies = parseCookies(req);
  const role = cookies.role && Object.values(USERS).some((u) => u.role === cookies.role) ? cookies.role : null;

  if (url.pathname === "/" ) {
    res.writeHead(302, { Location: role ? "/dashboard" : "/login" });
    return res.end();
  }

  if (url.pathname === "/login" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(
      page(
        "Log in",
        `<h1>Log in</h1>
         <form method="POST" action="/login">
           <input id="username" name="username" placeholder="Username" />
           <input id="password" name="password" type="password" placeholder="Password" />
           <button id="submit-login" type="submit">Log in</button>
         </form>`
      )
    );
  }

  if (url.pathname === "/login" && req.method === "POST") {
    const body = await readBody(req);
    const params = new URLSearchParams(body);
    const username = params.get("username") || "";
    const password = params.get("password") || "";
    const user = USERS[username];
    if (user && user.password === password) {
      res.writeHead(302, {
        "Set-Cookie": `role=${user.role}; Path=/`,
        Location: "/dashboard",
      });
      return res.end();
    }
    res.writeHead(401, { "Content-Type": "text/html" });
    return res.end(page("Log in failed", "<h1>Invalid credentials</h1>"));
  }

  if (url.pathname === "/logout") {
    res.writeHead(302, { "Set-Cookie": "role=; Path=/; Max-Age=0", Location: "/login" });
    return res.end();
  }

  if (!role) {
    res.writeHead(302, { Location: "/login" });
    return res.end();
  }

  if (url.pathname === "/dashboard") {
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(page("Dashboard", `<h1>Dashboard</h1>${nav(role)}<p>Welcome, ${role}.</p>`));
  }

  if (url.pathname === "/profile") {
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(page("Profile", `<h1>Profile</h1>${nav(role)}<p>Role: ${role}</p>`));
  }

  if (url.pathname === "/settings") {
    if (role !== "admin") {
      res.writeHead(403, { "Content-Type": "text/html" });
      return res.end(page("Forbidden", "<h1>403 Forbidden</h1>"));
    }
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(
      page("Settings", `<h1>Settings</h1>${nav(role)}<button id="danger-zone">Delete Account</button>`)
    );
  }

  if (url.pathname === "/users") {
    if (role !== "admin") {
      res.writeHead(403, { "Content-Type": "text/html" });
      return res.end(page("Forbidden", "<h1>403 Forbidden</h1>"));
    }
    res.writeHead(200, { "Content-Type": "text/html" });
    return res.end(
      page("Manage Users", `<h1>Manage Users</h1>${nav(role)}<button id="invite-user">Invite User</button>`)
    );
  }

  res.writeHead(404, { "Content-Type": "text/html" });
  res.end(page("Not found", "<h1>404</h1>"));
});

const port = process.env.FIXTURE_PORT ? Number(process.env.FIXTURE_PORT) : 4173;
server.listen(port, "127.0.0.1", () => {
  console.log(`role_gated_app fixture listening on http://127.0.0.1:${port}`);
});

module.exports = server;
