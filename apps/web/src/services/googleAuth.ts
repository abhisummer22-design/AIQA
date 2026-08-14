import { initializeApp, getApps, getApp } from "firebase/app";
import {
  getAuth,
  signInWithPopup,
  GoogleAuthProvider,
  onAuthStateChanged,
  signOut,
  User
} from "firebase/auth";
import firebaseConfig from "../../firebase-applet-config.json";

// Initialize Firebase App singleton
const app = getApps().length === 0 ? initializeApp(firebaseConfig) : getApp();
export const auth = getAuth(app);

// Provider with requested Workspace Scopes
export const provider = new GoogleAuthProvider();

export const WORKSPACE_SCOPES = [
  // Gmail Scopes
  "https://mail.google.com/",
  "https://www.googleapis.com/auth/gmail.send",
  "https://www.googleapis.com/auth/gmail.readonly",
  "https://www.googleapis.com/auth/gmail.compose",
  "https://www.googleapis.com/auth/gmail.modify",
  // Google Sheets & Drive Scopes
  "https://www.googleapis.com/auth/spreadsheets",
  "https://www.googleapis.com/auth/drive.file",
  "https://www.googleapis.com/auth/drive.readonly",
  // Google Calendar Scopes
  "https://www.googleapis.com/auth/calendar",
  "https://www.googleapis.com/auth/calendar.events"
];

WORKSPACE_SCOPES.forEach((scope) => {
  provider.addScope(scope);
});

// Always prompt for account selection / consent if needed
provider.setCustomParameters({
  prompt: "consent",
  access_type: "offline"
});

// Cache the access token in memory (never in localStorage/sessionStorage)
let cachedAccessToken: string | null = null;
let isSigningIn = false;

export interface AuthState {
  user: User | null;
  token: string | null;
  loading: boolean;
}

type AuthListener = (state: AuthState) => void;
const listeners: Set<AuthListener> = new Set();

function notifyListeners(state: AuthState) {
  listeners.forEach((listener) => {
    try {
      listener(state);
    } catch (e) {
      console.error("Auth listener error:", e);
    }
  });
}

export const subscribeAuth = (listener: AuthListener): (() => void) => {
  listeners.add(listener);
  // Emit current state immediately
  listener({
    user: auth.currentUser,
    token: cachedAccessToken,
    loading: false
  });
  return () => {
    listeners.delete(listener);
  };
};

export const initAuth = (
  onAuthSuccess?: (user: User, token: string) => void,
  onAuthFailure?: () => void
) => {
  return onAuthStateChanged(auth, async (user: User | null) => {
    if (user) {
      if (cachedAccessToken) {
        if (onAuthSuccess) onAuthSuccess(user, cachedAccessToken);
        notifyListeners({ user, token: cachedAccessToken, loading: false });
      } else if (!isSigningIn) {
        cachedAccessToken = null;
        if (onAuthFailure) onAuthFailure();
        notifyListeners({ user, token: null, loading: false });
      }
    } else {
      cachedAccessToken = null;
      if (onAuthFailure) onAuthFailure();
      notifyListeners({ user: null, token: null, loading: false });
    }
  });
};

export const googleSignIn = async (): Promise<{ user: User; accessToken: string } | null> => {
  try {
    isSigningIn = true;
    notifyListeners({ user: auth.currentUser, token: cachedAccessToken, loading: true });
    const result = await signInWithPopup(auth, provider);
    const credential = GoogleAuthProvider.credentialFromResult(result);
    if (!credential?.accessToken) {
      throw new Error("Failed to obtain OAuth access token from Google sign-in.");
    }

    cachedAccessToken = credential.accessToken;
    notifyListeners({ user: result.user, token: cachedAccessToken, loading: false });
    return { user: result.user, accessToken: cachedAccessToken };
  } catch (error: any) {
    console.error("Google Workspace Sign-in Error:", error);
    notifyListeners({ user: auth.currentUser, token: cachedAccessToken, loading: false });
    throw error;
  } finally {
    isSigningIn = false;
  }
};

export const getAccessToken = async (): Promise<string | null> => {
  return cachedAccessToken;
};

export const setAccessToken = (token: string | null) => {
  cachedAccessToken = token;
  notifyListeners({ user: auth.currentUser, token: cachedAccessToken, loading: false });
};

export const logoutGoogle = async () => {
  await signOut(auth);
  cachedAccessToken = null;
  notifyListeners({ user: null, token: null, loading: false });
};
