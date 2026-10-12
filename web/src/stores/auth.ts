import { create } from 'zustand';
import client from '../api/client';
import type { User, LoginResponse } from '../api/types';

interface AuthState {
  token: string | null;
  user: User | null;
  isAuthenticated: boolean;
  login: (username: string, password: string) => Promise<void>;
  logout: () => void;
  loadFromStorage: () => void;
}

// Read from localStorage synchronously at module load time to avoid flash-redirect
function getInitialAuth(): { token: string | null; user: User | null; isAuthenticated: boolean } {
  const token = localStorage.getItem('token');
  const userStr = localStorage.getItem('user');
  if (token && userStr) {
    try {
      const user = JSON.parse(userStr) as User;
      return { token, user, isAuthenticated: true };
    } catch {
      localStorage.removeItem('token');
      localStorage.removeItem('user');
    }
  }
  return { token: null, user: null, isAuthenticated: false };
}

const initial = getInitialAuth();

export const useAuthStore = create<AuthState>((set) => ({
  token: initial.token,
  user: initial.user,
  isAuthenticated: initial.isAuthenticated,

  login: async (username: string, password: string) => {
    const response = await client.post<LoginResponse>('/api/auth/login', {
      username,
      password,
    });
    const { token, user } = response.data;
    localStorage.setItem('token', token);
    localStorage.setItem('user', JSON.stringify(user));
    set({ token, user, isAuthenticated: true });
  },

  logout: () => {
    localStorage.removeItem('token');
    localStorage.removeItem('user');
    set({ token: null, user: null, isAuthenticated: false });
  },

  loadFromStorage: () => {
    const token = localStorage.getItem('token');
    const userStr = localStorage.getItem('user');
    if (token && userStr) {
      try {
        const user = JSON.parse(userStr) as User;
        set({ token, user, isAuthenticated: true });
      } catch {
        localStorage.removeItem('token');
        localStorage.removeItem('user');
        set({ token: null, user: null, isAuthenticated: false });
      }
    }
  },
}));
