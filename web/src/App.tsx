import { useEffect } from 'react';
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { ConfigProvider, App as AntApp } from 'antd';
import { useAuthStore } from '@/stores/auth';
import { AuthGuard } from '@/components/AuthGuard';
import AdminLayout from '@/layouts/AdminLayout';
import Login from '@/pages/Login';
import Dashboard from '@/pages/Dashboard';
import Providers from '@/pages/Providers';
import Models from '@/pages/Models';
import Organizations from '@/pages/Organizations';
import Users from '@/pages/Users';
import Keys from '@/pages/Keys';
import Logs from '@/pages/Logs';
import Settings from '@/pages/Settings';
import Playground from '@/pages/Playground';
import Quota from '@/pages/Quota';

export default function App() {
  const loadFromStorage = useAuthStore((s) => s.loadFromStorage);

  useEffect(() => {
    loadFromStorage();
  }, [loadFromStorage]);

  return (
    <ConfigProvider
      theme={{
        token: {
          colorPrimary: '#1677ff',
        },
      }}
    >
      <AntApp>
        <BrowserRouter basename={import.meta.env.BASE_URL}>
          <Routes>
            <Route path="/login" element={<Login />} />
            <Route
              element={
                <AuthGuard>
                  <AdminLayout />
                </AuthGuard>
              }
            >
              <Route path="/dashboard" element={<Dashboard />} />
              <Route path="/providers" element={<Providers />} />
              <Route path="/models" element={<Models />} />
              <Route path="/organizations" element={<Organizations />} />
              <Route path="/users" element={<Users />} />
              <Route path="/keys" element={<Keys />} />
              <Route path="/logs" element={<Logs />} />
              <Route path="/quota" element={<Quota />} />
              <Route path="/playground" element={<Playground />} />
              <Route path="/settings" element={<Settings />} />
            </Route>
            <Route path="/" element={<Navigate to="/dashboard" replace />} />
            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Routes>
        </BrowserRouter>
      </AntApp>
    </ConfigProvider>
  );
}
