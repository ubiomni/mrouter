import { Outlet, useNavigate, useLocation } from 'react-router-dom';
import { Layout, Menu, Button, Typography, Dropdown } from 'antd';
import {
  DashboardOutlined,
  CloudServerOutlined,
  AppstoreOutlined,
  UserOutlined,
  KeyOutlined,
  FileTextOutlined,
  SettingOutlined,
  LogoutOutlined,
  BankOutlined,
  GlobalOutlined,
  CodeOutlined,
  FundOutlined,
  ApiOutlined,
  MonitorOutlined,
} from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { useAuthStore } from '@/stores/auth';

const { Header, Sider, Content } = Layout;

export default function AdminLayout() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);

  const role = user?.role || 'user';

  const allMenuItems = [
    { key: '/dashboard', icon: <DashboardOutlined />, label: t('nav.dashboard'), roles: ['super_admin', 'org_admin', 'user'] },
    {
      key: 'channel',
      icon: <ApiOutlined />,
      label: t('nav.channelMgmt'),
      roles: ['super_admin', 'org_admin'],
      children: [
        { key: '/providers', icon: <CloudServerOutlined />, label: t('nav.providers'), roles: ['super_admin', 'org_admin'] },
        { key: '/models', icon: <AppstoreOutlined />, label: t('nav.models'), roles: ['super_admin', 'org_admin'] },
      ],
    },
    {
      key: 'user',
      icon: <UserOutlined />,
      label: t('nav.userMgmt'),
      roles: ['super_admin', 'org_admin', 'user'],
      children: [
        { key: '/organizations', icon: <BankOutlined />, label: t('nav.organizations'), roles: ['super_admin', 'org_admin'] },
        { key: '/users', icon: <UserOutlined />, label: t('nav.users'), roles: ['super_admin', 'org_admin'] },
        { key: '/keys', icon: <KeyOutlined />, label: t('nav.keys'), roles: ['super_admin', 'org_admin', 'user'] },
      ],
    },
    {
      key: 'ops',
      icon: <MonitorOutlined />,
      label: t('nav.opsMgmt'),
      roles: ['super_admin', 'org_admin', 'user'],
      children: [
        { key: '/logs', icon: <FileTextOutlined />, label: t('nav.logs'), roles: ['super_admin', 'org_admin', 'user'] },
        { key: '/quota', icon: <FundOutlined />, label: t('nav.quota'), roles: ['super_admin', 'org_admin', 'user'] },
        { key: '/playground', icon: <CodeOutlined />, label: t('nav.playground'), roles: ['super_admin', 'org_admin', 'user'] },
      ],
    },
    {
      key: 'system',
      icon: <SettingOutlined />,
      label: t('nav.system'),
      roles: ['super_admin', 'org_admin'],
      children: [
        { key: '/settings', icon: <SettingOutlined />, label: t('nav.settings'), roles: ['super_admin', 'org_admin'] },
      ],
    },
  ];

  const menuItems = allMenuItems
    .filter(item => item.roles.includes(role))
    .map(item => item.children
      ? { ...item, children: item.children.filter(c => c.roles.includes(role)) }
      : item
    )
    .filter(item => !item.children || item.children.length > 0);

  // Determine which submenu should be open based on current path
  const getOpenKeys = () => {
    const path = location.pathname;
    if (['/providers', '/models'].includes(path)) return ['channel'];
    if (['/organizations', '/users', '/keys'].includes(path)) return ['user'];
    if (['/logs', '/quota', '/playground'].includes(path)) return ['ops'];
    if (['/settings'].includes(path)) return ['system'];
    return [];
  };

  const handleLogout = () => {
    logout();
    navigate('/login', { replace: true });
  };

  const switchLang = (lang: string) => {
    i18n.changeLanguage(lang);
    localStorage.setItem('i18n_lang', lang);
  };

  const langMenuItems = [
    { key: 'zh', label: '中文', onClick: () => switchLang('zh') },
    { key: 'en', label: 'English', onClick: () => switchLang('en') },
  ];

  const userMenuItems = [
    {
      key: 'logout',
      icon: <LogoutOutlined />,
      label: t('nav.logout'),
      onClick: handleLogout,
    },
  ];

  return (
    <Layout style={{ height: '100vh', overflow: 'hidden' }}>
      <Sider theme="dark" width={200} style={{ overflow: 'auto' }}>
        <div
          style={{
            height: 48,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            borderBottom: '1px solid rgba(255,255,255,0.1)',
          }}
        >
          <Typography.Text
            strong
            style={{ color: '#fff', fontSize: 16 }}
          >
            MRouter
          </Typography.Text>
        </div>
        <Menu
          theme="dark"
          mode="inline"
          selectedKeys={[location.pathname]}
          defaultOpenKeys={getOpenKeys()}
          items={menuItems}
          onClick={({ key }) => {
            if (key.startsWith('/')) navigate(key)
          }}
        />
      </Sider>
      <Layout style={{ overflow: 'hidden' }}>
        <Header
          style={{
            background: '#fff',
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'flex-end',
            borderBottom: '1px solid #f0f0f0',
            gap: 8,
            flex: '0 0 48px',
            height: 48,
            lineHeight: '48px',
          }}
        >
          <Dropdown menu={{ items: langMenuItems }} placement="bottomRight">
            <Button type="text" icon={<GlobalOutlined />}>
              {i18n.language === 'zh' ? '中文' : 'EN'}
            </Button>
          </Dropdown>
          <Dropdown menu={{ items: userMenuItems }} placement="bottomRight">
            <Button type="text">
              <UserOutlined /> {user?.display_name || user?.username}{' '}
              <Typography.Text type="secondary" style={{ marginLeft: 4 }}>
                ({user?.role})
              </Typography.Text>
            </Button>
          </Dropdown>
        </Header>
        <Content style={{ margin: 12, overflow: 'auto' }}>
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}
