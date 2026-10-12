import axios from 'axios';

// Resolve API base from vite's base path (e.g. '/modelrouter/' in production)
// In dev mode, BASE_URL is '/' and vite proxy handles /api → backend
const BASE_URL = import.meta.env.VITE_API_BASE_URL
  || (import.meta.env.BASE_URL !== '/' ? import.meta.env.BASE_URL.replace(/\/$/, '') : '');

const client = axios.create({
  baseURL: BASE_URL,
  timeout: 30000,
  headers: {
    'Content-Type': 'application/json',
  },
});

client.interceptors.request.use((config) => {
  const token = localStorage.getItem('token');
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

client.interceptors.response.use(
  (response) => {
    // Unwrap the API envelope: {code, data, message} → data
    if (response.data && typeof response.data === 'object' && 'code' in response.data && 'data' in response.data) {
      response.data = response.data.data;
    }
    return response;
  },
  (error) => {
    if (error.response?.status === 401) {
      localStorage.removeItem('token');
      localStorage.removeItem('user');
      const base = import.meta.env.BASE_URL || '/';
      window.location.href = `${base}login`;
    }
    return Promise.reject(error);
  }
);

export default client;
