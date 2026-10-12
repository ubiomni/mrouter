import React, { useState, useEffect, useCallback } from 'react';
import {
  Table, Modal, Form, Input, Select, Switch, InputNumber,
  Button, Tag, Space, Card, Typography, Descriptions, Popconfirm, Tooltip, App, Radio,
} from 'antd';
import {
  SearchOutlined, PlayCircleOutlined, PlusOutlined, EditOutlined, DeleteOutlined,
  CheckCircleOutlined, CloseCircleOutlined,
} from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import type { ColumnsType } from 'antd/es/table';
import {
  Provider, ProviderStatus, ProviderPreset,
  createProvider, updateProvider, deleteProvider,
  getProviderPresets, activateProvider, deactivateProvider,
} from '../api/providers';
import { listSearchProviders, testSearch } from '../api/search';

const { Text } = Typography;

export default function SearchGateway() {
  const { message } = App.useApp();
  const { t } = useTranslation();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(false);
  const [presets, setPresets] = useState<ProviderPreset[]>([]);

  // Create/Edit modal
  const [modalOpen, setModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);
  const [form] = Form.useForm();

  // Test modal
  const [testModalOpen, setTestModalOpen] = useState(false);
  const [testingProvider, setTestingProvider] = useState<Provider | null>(null);
  const [testResult, setTestResult] = useState<any>(null);
  const [testLoading, setTestLoading] = useState(false);
  const [testForm] = Form.useForm();

  const searchPresets = presets.filter(p => p.category === 'search');

  const fetchProviders = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listSearchProviders();
      setProviders(res.data || []);
    } catch {
      message.error('Failed to load search providers');
    } finally {
      setLoading(false);
    }
  }, []);

  const fetchPresets = useCallback(async () => {
    try {
      const res = await getProviderPresets();
      setPresets(res.data || []);
    } catch { /* non-critical */ }
  }, []);

  useEffect(() => { fetchProviders(); }, [fetchProviders]);
  useEffect(() => { fetchPresets(); }, [fetchPresets]);

  // --- CRUD ---

  const openCreateModal = () => {
    setEditingProvider(null);
    form.resetFields();
    form.setFieldsValue({
      priority: 0,
      weight: 100,
      status: ProviderStatus.Enabled,
      rpm_limit: 0,
    });
    setModalOpen(true);
  };

  const openEditModal = (record: Provider) => {
    setEditingProvider(record);
    form.resetFields();
    form.setFieldsValue({
      name: record.name,
      display_name: record.display_name,
      preset_key: record.preset_key,
      base_url: record.base_url,
      api_key: '',
      auth_method: record.auth_method || 'bearer',
      priority: record.priority,
      weight: record.weight,
      status: record.status,
      rpm_limit: record.rpm_limit || 0,
    });
    setModalOpen(true);
  };

  const handlePresetChange = (presetKey: string | undefined) => {
    const preset = presetKey ? searchPresets.find(p => p.preset_key === presetKey) : null;
    if (preset) {
      form.setFieldsValue({
        name: preset.preset_key,
        base_url: preset.base_url || '',
        auth_method: preset.auth_method || 'bearer',
      });
    }
  };

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields();
      const payload: any = {
        name: values.name,
        display_name: values.display_name || '',
        preset_key: values.preset_key || '',
        base_url: values.base_url,
        auth_method: values.auth_method || 'bearer',
        priority: values.priority ?? 0,
        weight: values.weight ?? 100,
        status: values.status ?? 1,
        rpm_limit: values.rpm_limit ?? 0,
        provider_group: 'search',
        category: 'search',
      };
      if (values.api_key) {
        payload.api_key = values.api_key;
      }

      if (editingProvider) {
        await updateProvider(editingProvider.id, payload);
        message.success(t('providers.updated', 'Updated'));
      } else {
        await createProvider(payload);
        message.success(t('providers.created', 'Created'));
      }
      setModalOpen(false);
      fetchProviders();
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error(err?.response?.data?.message || t('common.failed', 'Failed'));
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteProvider(id);
      message.success(t('providers.deleted', 'Deleted'));
      fetchProviders();
    } catch {
      message.error(t('common.failed', 'Failed'));
    }
  };

  const handleToggleActive = async (id: number, active: boolean) => {
    try {
      if (active) {
        await activateProvider(id);
      } else {
        await deactivateProvider(id);
      }
      fetchProviders();
    } catch {
      message.error(t('common.failed', 'Failed'));
    }
  };

  // --- Test ---

  const getTestDefaults = (provider: Provider) => {
    const key = provider.preset_key || provider.name || '';
    if (key.includes('baidu')) {
      return {
        body: JSON.stringify({"messages":[{"role":"user","content":"今天热点新闻"}],"edition":"standard","search_source":"baidu_search_v2"}, null, 2),
      };
    }
    if (key.includes('searchapi')) {
      return {
        body: JSON.stringify({"engine":"google","q":"hello world"}, null, 2),
      };
    }
    if (key.includes('exa')) {
      return {
        body: JSON.stringify({"query":"hello world","type":"auto","numResults":5}, null, 2),
      };
    }
    if (key.includes('tavily')) {
      return {
        body: JSON.stringify({"query":"hello world","search_depth":"basic","max_results":5}, null, 2),
      };
    }
    if (key.includes('bocha')) {
      return {
        body: JSON.stringify({"query":"hello world"}, null, 2),
      };
    }
    return {
      body: JSON.stringify({"query":"hello world"}, null, 2),
    };
  };

  const openTestModal = (provider: Provider) => {
    setTestingProvider(provider);
    setTestResult(null);
    const defaults = getTestDefaults(provider);
    testForm.setFieldsValue({ body: defaults.body });
    setTestModalOpen(true);
  };

  const handleTest = async () => {
    if (!testingProvider) return;
    setTestLoading(true);
    setTestResult(null);
    try {
      const values = await testForm.validateFields();
      let bodyObj: any;
      try {
        bodyObj = JSON.parse(values.body);
      } catch {
        message.error('Invalid JSON body');
        setTestLoading(false);
        return;
      }
      const res = await testSearch({
        provider_id: testingProvider.id,
        body: bodyObj,
      });
      setTestResult(res.data);
    } catch (e: any) {
      message.error(e.response?.data?.message || 'Test failed');
    } finally {
      setTestLoading(false);
    }
  };

  // --- Table ---

  const columns: ColumnsType<Provider> = [
    { title: 'ID', dataIndex: 'id', width: 60 },
    {
      title: t('providers.name', 'Name'),
      dataIndex: 'name',
      render: (name: string, record: Provider) => (
        <div>
          <Text strong>{name}</Text>
          {record.display_name && (
            <div><Text type="secondary" style={{ fontSize: 12 }}>{record.display_name}</Text></div>
          )}
        </div>
      ),
    },
    {
      title: 'Preset',
      dataIndex: 'preset_key',
      width: 140,
      render: (v: string | null) => v ? <Tag color="blue">{v}</Tag> : '-',
    },
    {
      title: 'Base URL',
      dataIndex: 'base_url',
      ellipsis: true,
      width: 240,
      render: (url: string) => (
        <Tooltip title={url}><Text style={{ fontSize: 12 }}>{url}</Text></Tooltip>
      ),
    },
    {
      title: t('providers.status', 'Status'),
      dataIndex: 'status',
      width: 100,
      render: (status: ProviderStatus, record: Provider) => (
        <Switch
          checked={status === ProviderStatus.Enabled}
          size="small"
          checkedChildren={<CheckCircleOutlined />}
          unCheckedChildren={<CloseCircleOutlined />}
          onChange={(checked) => handleToggleActive(record.id, checked)}
        />
      ),
    },
    {
      title: t('common.actions', 'Actions'),
      width: 200,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<PlayCircleOutlined />} onClick={() => openTestModal(record)}>
            Test
          </Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEditModal(record)} />
          <Popconfirm title="Delete this provider?" onConfirm={() => handleDelete(record.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <Card
        title={
          <Space>
            <SearchOutlined />
            <span>{t('nav.search', 'Search Gateway')}</span>
          </Space>
        }
        extra={
          <Space>
            <Text type="secondary" style={{ fontSize: 12 }}>
              POST /v1/search/&lt;path&gt;
            </Text>
            <Button type="primary" icon={<PlusOutlined />} onClick={openCreateModal}>
              {t('providers.add', 'Add')}
            </Button>
          </Space>
        }
      >
        <Descriptions column={2} size="small" style={{ marginBottom: 16 }}>
          <Descriptions.Item label="Endpoint">
            <Text code>POST /v1/search/{'<path>'}</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Provider Selection">
            <Text code>X-Provider-Name</Text> header or auto-select first active
          </Descriptions.Item>
        </Descriptions>

        <Table
          rowKey="id"
          columns={columns}
          dataSource={providers}
          loading={loading}
          pagination={false}
          size="middle"
        />
      </Card>

      {/* Create/Edit Modal */}
      <Modal
        title={editingProvider ? t('providers.edit', 'Edit Search Provider') : t('providers.add', 'Add Search Provider')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={handleSubmit}
        width={560}
        destroyOnClose
      >
        <Form form={form} layout="vertical" size="middle">
          <Form.Item name="preset_key" label="Preset">
            <Select
              placeholder="Select a search preset"
              onChange={handlePresetChange}
              showSearch
              optionFilterProp="label"
              allowClear
              options={searchPresets.map(p => ({ value: p.preset_key, label: p.display_name || p.preset_key }))}
            />
          </Form.Item>
          <Form.Item name="name" label={t('providers.name', 'Name')} rules={[{ required: true }]}>
            <Input placeholder="e.g. tavily-main" />
          </Form.Item>
          <Form.Item name="display_name" label="Display Name">
            <Input placeholder="Tavily Search (optional)" />
          </Form.Item>
          <Form.Item name="base_url" label="Base URL" rules={[{ required: true }]}>
            <Input placeholder="https://api.tavily.com" />
          </Form.Item>
          <Form.Item name="api_key" label="API Key" extra={editingProvider ? 'Leave blank to keep existing key' : ''}>
            <Input.Password placeholder="sk-..." />
          </Form.Item>
          <Form.Item name="auth_method" label="Auth Method" initialValue="bearer">
            <Select options={[
              { value: 'bearer', label: 'Bearer Token' },
              { value: 'header', label: 'Custom Header' },
              { value: 'query', label: 'Query Parameter' },
              { value: 'none', label: 'None' },
            ]} />
          </Form.Item>
          <Space size="large" align="start">
            <Form.Item name="priority" label="Priority">
              <InputNumber min={0} style={{ width: 100 }} />
            </Form.Item>
            <Form.Item name="weight" label="Weight">
              <InputNumber min={1} style={{ width: 100 }} />
            </Form.Item>
            <Form.Item name="rpm_limit" label="RPM Limit" tooltip="0 = unlimited">
              <InputNumber min={0} style={{ width: 100 }} />
            </Form.Item>
          </Space>
          <Form.Item name="status" label={t('providers.active', 'Active')} initialValue={ProviderStatus.Enabled}>
            <Radio.Group>
              <Radio value={ProviderStatus.Enabled}>{t('providers.statusEnabled', 'Enabled')}</Radio>
              <Radio value={ProviderStatus.ManuallyDisabled}>{t('providers.statusDisabled', 'Disabled')}</Radio>
            </Radio.Group>
          </Form.Item>
        </Form>
      </Modal>

      {/* Test Modal */}
      <Modal
        title={`Test Search — ${testingProvider?.name || ''}`}
        open={testModalOpen}
        onCancel={() => setTestModalOpen(false)}
        footer={null}
        width={700}
      >
        <Form form={testForm} layout="vertical">
          <Form.Item label="Request Body (JSON)" name="body" rules={[{ required: true }]}>
            <Input.TextArea rows={6} style={{ fontFamily: 'monospace', fontSize: 12 }} />
          </Form.Item>
          <Button type="primary" loading={testLoading} onClick={handleTest} icon={<SearchOutlined />}>
            Send Test
          </Button>
        </Form>

        {testResult && (
          <Card size="small" style={{ marginTop: 16 }}>
            <Descriptions column={2} size="small">
              <Descriptions.Item label="Status">
                <Tag color={testResult.status < 400 ? 'success' : 'error'}>{testResult.status}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Provider">{testResult.provider}</Descriptions.Item>
            </Descriptions>
            <pre style={{ maxHeight: 400, overflow: 'auto', fontSize: 12, marginTop: 8, background: '#f5f5f5', padding: 12, borderRadius: 4 }}>
              {JSON.stringify(testResult.response, null, 2)}
            </pre>
          </Card>
        )}
      </Modal>
    </div>
  );
}
