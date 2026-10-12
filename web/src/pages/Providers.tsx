import React, { useState, useEffect, useCallback } from 'react';
import {
  Table, Modal, Form, Input, Select, Switch, InputNumber,
  Button, Tag, Space, Popconfirm, Typography, Collapse,
  Radio, Tooltip, Upload, App, Segmented,
} from 'antd';
import {
  PlusOutlined, EditOutlined, DeleteOutlined,
  CheckCircleOutlined, CloseCircleOutlined, DownloadOutlined,
  UploadOutlined, ThunderboltOutlined, ReloadOutlined, MinusCircleOutlined,
  SearchOutlined, PlayCircleOutlined,
} from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import type { ColumnsType } from 'antd/es/table';
import {
  Provider, ProviderStatus, ProviderPreset, PresetEndpoint, ProviderEndpointItem,
  listProviders, exportProviders, importProviders, createProvider, updateProvider, deleteProvider,
  getProviderPresets, testProvider, activateProvider, deactivateProvider, resetCircuitBreaker,
  fetchProviderModels, fetchModelsAdhoc,
  listProviderEndpoints, createProviderEndpoint, deleteProviderEndpoint,
} from '../api/providers';
import { createProviderModel, listProviderModels, deleteProviderModel } from '../api/models';
import { testSearch } from '../api/search';

const { TextArea } = Input;
const { Text } = Typography;



export default function Providers() {
  const { message } = App.useApp();
  const { t } = useTranslation();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [allProviders, setAllProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(false);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [categoryFilter, setCategoryFilter] = useState<string>('all');
  const [modalOpen, setModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Provider | null>(null);
  const [presets, setPresets] = useState<ProviderPreset[]>([]);
  const [selectedPreset, setSelectedPreset] = useState<ProviderPreset | null>(null);
  const [testingIds, setTestingIds] = useState<Set<number>>(new Set());
  const [testResults, setTestResults] = useState<Record<number, { ok: boolean; msg: string }>>({});
  const [modalTesting, setModalTesting] = useState(false);
  const [fetchingModels, setFetchingModels] = useState(false);
  const [providerEndpoints, setProviderEndpoints] = useState<Record<number, ProviderEndpointItem[]>>({});
  const [addEpModalOpen, setAddEpModalOpen] = useState(false);
  const [addEpProviderId, setAddEpProviderId] = useState<number>(0);
  const [addEpForm] = Form.useForm();
  const [form] = Form.useForm();
  // Search test modal
  const [searchTestModalOpen, setSearchTestModalOpen] = useState(false);
  const [searchTestProvider, setSearchTestProvider] = useState<Provider | null>(null);
  const [searchTestResult, setSearchTestResult] = useState<any>(null);
  const [searchTestLoading, setSearchTestLoading] = useState(false);
  const [searchTestForm] = Form.useForm();

  // Helper: get the primary format from preset endpoints (most common format)
  const getPresetFormat = (preset: ProviderPreset | null): string => {
    if (!preset || !preset.endpoints.length) return 'openai';
    const formats = preset.endpoints.map(e => e.format);
    return formats[0] || 'openai';
  };
  const fetchProviders = useCallback(async () => {
    setLoading(true);
    try {
      // Fetch all providers; category filtering is client-side
      const res = await listProviders(1, 1000);
      const all = res.data?.elements || res.data || [];
      setAllProviders(Array.isArray(all) ? all : []);
    } catch {
      message.error(t('providers.loadFailed'));
    } finally {
      setLoading(false);
    }
  }, []);

  // Apply category filter client-side
  useEffect(() => {
    const filtered = categoryFilter === 'all' ? allProviders
      : categoryFilter === 'search' ? allProviders.filter((p: Provider) => p.category === 'search' || p.provider_group === 'search')
      : allProviders.filter((p: Provider) => p.category !== 'search' && p.provider_group !== 'search');
    setProviders(filtered);
    setTotal(filtered.length);
    setPage(1);
  }, [allProviders, categoryFilter]);

  const fetchPresets = useCallback(async () => {
    try {
      const res = await getProviderPresets();
      setPresets(res.data || []);
    } catch {
      // Presets may not be available, non-critical
    }
  }, []);

  useEffect(() => { fetchProviders(); }, [fetchProviders]);
  useEffect(() => { fetchPresets(); }, [fetchPresets]);

  const fetchEndpoints = async (providerId: number) => {
    try {
      const res = await listProviderEndpoints(providerId);
      setProviderEndpoints(prev => ({ ...prev, [providerId]: res.data || [] }));
    } catch { /* ignore */ }
  };

  const handleAddEndpoint = async (providerId: number, ep: { endpoint_type: string; api_format: string; endpoint_path: string }) => {
    try {
      await createProviderEndpoint(providerId, ep);
      message.success('Endpoint added');
      fetchEndpoints(providerId);
      setAddEpModalOpen(false);
    } catch {
      message.error('Failed to add endpoint');
    }
  };

  const openAddEndpointModal = (providerId: number) => {
    setAddEpProviderId(providerId);
    addEpForm.resetFields();
    addEpForm.setFieldsValue({ endpoint_type: 'chat', api_format: 'openai', endpoint_path: '/v1/chat/completions' });
    setAddEpModalOpen(true);
  };

  const handleAddEpSubmit = async () => {
    const vals = await addEpForm.validateFields();
    await handleAddEndpoint(addEpProviderId, vals);
  };

  const handleDeleteEndpoint = async (providerId: number, endpointId: number) => {
    try {
      await deleteProviderEndpoint(endpointId);
      message.success('Endpoint removed');
      fetchEndpoints(providerId);
    } catch {
      message.error('Failed to delete endpoint');
    }
  };

  const openCreateModal = () => {
    setEditingProvider(null);
    setSelectedPreset(null);
    form.resetFields();
    form.setFieldsValue({
      provider_group: 'default',
      category: categoryFilter === 'search' ? 'search' : 'llm',
      priority: 0,
      weight: 100,
      status: ProviderStatus.Enabled,
      auto_ban: true,
      rpm_limit: -1,
      tpm_limit: -1,
      concurrent_limit: -1,
    });
    setModalOpen(true);
  };

  const openEditModal = async (record: Provider) => {
    setEditingProvider(record);
    // Match preset by preset_key field, fallback to name
    const preset = presets.find(p => p.preset_key === record.preset_key)
      || presets.find(p => p.preset_key === record.name)
      || null;
    setSelectedPreset(preset);
    // Reset form first to clear stale state
    form.resetFields();
    // Parse custom_headers JSON into key-value list
    let headersList: { key: string; value: string }[] = [];
    if (record.custom_headers) {
      try {
        // Backend returns custom_headers as object (not string)
        const parsed = typeof record.custom_headers === 'string'
          ? JSON.parse(record.custom_headers)
          : record.custom_headers;
        headersList = Object.entries(parsed).map(([k, v]) => ({ key: k, value: v as string }));
      } catch { /* ignore */ }
    }
    // Load existing endpoints for this provider
    let endpointsList: { endpoint_type: string; api_format: string; endpoint_path: string }[] = [];
    try {
      const epRes = await listProviderEndpoints(record.id);
      const eps: ProviderEndpointItem[] = epRes.data || [];
      endpointsList = eps.filter(ep => ep.enabled).map(ep => ({
        endpoint_type: ep.endpoint_type,
        api_format: ep.api_format,
        endpoint_path: ep.endpoint_path,
      }));
    } catch { /* ignore */ }
    // Load existing model associations
    let modelsText = '';
    try {
      const modelsRes = await listProviderModels(record.id);
      const models: any[] = modelsRes.data || [];
      modelsText = models.map((m: any) => m.model_pattern).filter(Boolean).join('\n');
    } catch { /* ignore */ }
    form.setFieldsValue({
      ...record,
      api_key: '',
      preset_key: preset?.preset_key || record.preset_key || undefined,
      custom_headers_list: headersList,
      endpoints_list: endpointsList,
      model_patterns: modelsText,
    });
    setModalOpen(true);
  };

  const handlePresetChange = (presetKey: string | undefined) => {
    const preset = presetKey ? presets.find(p => p.preset_key === presetKey) || null : null;
    setSelectedPreset(preset);
    if (preset) {
      const current = form.getFieldsValue();
      const updates: Record<string, any> = {};
      // Only fill fields that are currently empty
      if (!current.base_url && preset.base_url) {
        updates.base_url = preset.base_url;
      }
      if (!current.auth_method && preset.auth_method) {
        updates.auth_method = preset.auth_method;
      }
      if ((!current.endpoints_list || current.endpoints_list.length === 0) && preset.endpoints.length > 0) {
        updates.endpoints_list = preset.endpoints.map(ep => ({
          endpoint_type: ep.endpoint_type,
          api_format: ep.format,
          endpoint_path: ep.path,
        }));
      }
      if (Object.keys(updates).length > 0) {
        form.setFieldsValue(updates);
      }
    }
  };

  const handleRegionChange = (_regionName: string) => {
    // Regions are deprecated in endpoint-driven architecture
  };

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields();
      const { model_patterns, custom_headers_list, endpoints_list, ...payload } = values;
      // Convert key-value list to JSON object (NOT string — backend expects serde_json::Value)
      if (custom_headers_list && custom_headers_list.length > 0) {
        const headersObj: Record<string, string> = {};
        for (const item of custom_headers_list) {
          if (item.key) headersObj[item.key] = item.value || '';
        }
        payload.custom_headers = headersObj; // Send as object
      } else {
        payload.custom_headers = null;
      }
      // Don't send empty api_key on edit
      if (editingProvider && !payload.api_key) {
        delete payload.api_key;
      }

      // Ensure category is set (form field takes precedence)
      if (!payload.category) {
        payload.category = categoryFilter === 'search' ? 'search' : 'llm';
      }
      if (payload.category === 'search') {
        payload.provider_group = payload.provider_group || 'search';
      }

      // Attach endpoints to the create request body so backend handles it in one shot
      if (!editingProvider && endpoints_list && endpoints_list.length > 0) {
        payload.endpoints = endpoints_list.filter(
          (ep: any) => ep.endpoint_type && ep.api_format && ep.endpoint_path
        );
      }

      let providerId: number;
      if (editingProvider) {
        await updateProvider(editingProvider.id, payload);
        providerId = editingProvider.id;

        // Sync endpoints on edit: delete all existing, recreate from form
        if (endpoints_list) {
          try {
            const existingRes = await listProviderEndpoints(providerId);
            const existing: ProviderEndpointItem[] = existingRes.data || [];
            for (const ep of existing) {
              await deleteProviderEndpoint(ep.id);
            }
          } catch { /* ignore */ }
          const validEndpoints = (endpoints_list || []).filter(
            (ep: any) => ep.endpoint_type && ep.api_format && ep.endpoint_path
          );
          for (const ep of validEndpoints) {
            try {
              await createProviderEndpoint(providerId, ep);
            } catch { /* skip */ }
          }
        }

        message.success(t('providers.updated'));
      } else {
        const res = await createProvider(payload);
        providerId = res.data?.id || 0;
        message.success(t('providers.created'));
      }

      // Batch create/update model associations if model_patterns provided
      if (model_patterns && providerId) {
        const models = model_patterns.split('\n').map((m: string) => m.trim()).filter(Boolean);

        // On edit: sync models (delete removed, add new)
        if (editingProvider) {
          try {
            const existingRes = await listProviderModels(providerId);
            const existing: any[] = existingRes.data || [];
            const existingPatterns = existing.map((m: any) => m.model_pattern);
            const newPatterns = models;

            // Delete models that are no longer in the list
            for (const existingModel of existing) {
              if (!newPatterns.includes(existingModel.model_pattern)) {
                try {
                  await deleteProviderModel(existingModel.id);
                } catch { /* skip */ }
              }
            }

            // Add new models
            for (const modelPattern of newPatterns) {
              if (!existingPatterns.includes(modelPattern)) {
                try {
                  await createProviderModel(providerId, { model_pattern: modelPattern, enabled: true });
                } catch { /* skip duplicates */ }
              }
            }
          } catch { /* ignore */ }
        } else {
          // On create: just add all
          for (const modelPattern of models) {
            try {
              await createProviderModel(providerId, { model_pattern: modelPattern, enabled: true });
            } catch { /* skip duplicates */ }
          }
        }

        if (models.length > 0 && !editingProvider) {
          message.success(`Created ${models.length} model association(s)`);
        }
      }

      setModalOpen(false);
      fetchProviders();
    } catch (err: any) {
      if (err?.errorFields) return; // validation error
      message.error(err?.response?.data?.message || t('common.failed'));
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteProvider(id);
      message.success(t('providers.deleted'));
      fetchProviders();
    } catch {
      message.error(t('providers.deleteFailed'));
    }
  };

  const handleTest = async (id: number) => {
    setTestingIds(prev => new Set(prev).add(id));
    try {
      const res = await testProvider(id);
      const data = res.data;
      if (data?.success) {
        const latency = data?.latency_ms;
        setTestResults(prev => ({
          ...prev,
          [id]: { ok: true, msg: latency ? `${latency}ms` : 'OK' },
        }));
      } else {
        setTestResults(prev => ({
          ...prev,
          [id]: { ok: false, msg: data?.error || `HTTP ${data?.status_code || 'error'}` },
        }));
      }
    } catch (err: any) {
      setTestResults(prev => ({
        ...prev,
        [id]: { ok: false, msg: err?.response?.data?.message || 'Failed' },
      }));
    } finally {
      setTestingIds(prev => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const handleModalTest = async () => {
    setModalTesting(true);
    try {
      const values = await form.validateFields();
      // If editing, test against existing provider
      if (editingProvider) {
        await handleTest(editingProvider.id);
      } else {
        message.info('Save the provider first to test the connection');
      }
    } catch {
      // validation error
    } finally {
      setModalTesting(false);
    }
  };

  // --- Search Test ---
  const getSearchTestDefaults = (provider: Provider) => {
    const key = provider.preset_key || provider.name || '';
    if (key.includes('baidu')) {
      return JSON.stringify({"messages":[{"role":"user","content":"今天热点新闻"}],"edition":"standard","search_source":"baidu_search_v2"}, null, 2);
    }
    if (key.includes('tavily')) {
      return JSON.stringify({"query":"hello world","search_depth":"basic","max_results":5}, null, 2);
    }
    if (key.includes('exa')) {
      return JSON.stringify({"query":"hello world","type":"auto","numResults":5}, null, 2);
    }
    return JSON.stringify({"query":"hello world"}, null, 2);
  };

  const openSearchTestModal = (provider: Provider) => {
    setSearchTestProvider(provider);
    setSearchTestResult(null);
    searchTestForm.setFieldsValue({ body: getSearchTestDefaults(provider) });
    setSearchTestModalOpen(true);
  };

  const handleSearchTest = async () => {
    if (!searchTestProvider) return;
    setSearchTestLoading(true);
    setSearchTestResult(null);
    try {
      const values = await searchTestForm.validateFields();
      const bodyObj = JSON.parse(values.body);
      const res = await testSearch({ provider_id: searchTestProvider.id, body: bodyObj });
      setSearchTestResult(res.data);
    } catch (e: any) {
      message.error(e.response?.data?.message || 'Test failed');
    } finally {
      setSearchTestLoading(false);
    }
  };

  const isSearchProvider = (record: Provider) => record.category === 'search' || record.provider_group === 'search';

  const handleToggleActive = async (id: number, active: boolean) => {
    try {
      if (active) {
        await activateProvider(id);
      } else {
        await deactivateProvider(id);
      }
      message.success(t('common.success'));
      fetchProviders();
    } catch {
      message.error(t('common.failed'));
    }
  };

  const handleResetCircuitBreaker = async (id: number) => {
    try {
      await resetCircuitBreaker(id);
      message.success(t('providers.resetSuccess'));
    } catch {
      message.error(t('common.failed'));
    }
  };

  const handleFillDefaultModels = () => {
    // Default models are no longer stored in presets (endpoint-driven architecture)
    message.info('Use "Fetch from upstream" to discover available models');
  };

  const handleFetchUpstreamModels = async () => {
    setFetchingModels(true);
    try {
      let res;
      if (editingProvider) {
        // Existing provider — use its saved config
        res = await fetchProviderModels(editingProvider.id);
      } else {
        // New provider — use form values directly
        const values = form.getFieldsValue();
        const baseUrl = values.base_url;
        const apiKey = values.api_key;
        if (!baseUrl) {
          message.warning('Please fill in Base URL first');
          setFetchingModels(false);
          return;
        }
        res = await fetchModelsAdhoc({ base_url: baseUrl, api_key: apiKey || '' });
      }
      const models: string[] = res.data?.models || [];
      if (models.length === 0) {
        message.warning(res.data?.error || t('providers.noModelsFound'));
        return;
      }
      const modelsText = models.join('\n');
      form.setFieldsValue({ model_patterns: modelsText });
      message.success(`Fetched ${models.length} model(s) from upstream`);
    } catch (err: any) {
      message.error(err?.response?.data?.message || t('providers.fetchModelsFailed'));
    } finally {
      setFetchingModels(false);
    }
  };

  const handleExport = async () => {
    try {
      const res = await exportProviders();
      const data = res.data?.data || res.data || [];
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'providers-export.json';
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      message.error('Export failed');
    }
  };

  const handleImport = (file: any) => {
    const reader = new FileReader();
    reader.onload = async (e) => {
      try {
        const data = JSON.parse(e.target?.result as string);
        const items = Array.isArray(data) ? data : [data];
        // Use batch import API — handles endpoints + models
        const res = await importProviders(items);
        const result = res.data || {};
        message.success(`Imported: ${result.created || 0} created, ${result.updated || 0} updated`);
        fetchProviders();
      } catch {
        message.error(t('common.failed'));
      }
    };
    reader.readAsText(file);
    return false;
  };

  const columns: ColumnsType<Provider> = [
    {
      title: t('providers.name'),
      dataIndex: 'name',
      key: 'name',
      width: 180,
      ellipsis: { showTitle: false },
      render: (name: string, record: Provider) => (
        <Tooltip title={name}>
          <div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            <Text strong>{name}</Text>
          </div>
          {record.display_name && (
            <div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              <Text type="secondary" style={{ fontSize: 12 }}>{record.display_name}</Text>
            </div>
          )}
        </Tooltip>
      ),
    },
    {
      title: t('providers.category', 'Category'),
      dataIndex: 'category',
      key: 'category',
      width: 90,
      render: (cat: string) => {
        const color = cat === 'search' ? 'orange' : cat === 'embedding' ? 'purple' : 'green';
        return <Tag color={color}>{cat || 'llm'}</Tag>;
      },
    },
    {
      title: t('providers.auth', 'Auth'),
      dataIndex: 'auth_method',
      key: 'auth_method',
      width: 120,
      ellipsis: true,
      render: (method: string) => <Tag style={{ margin: 0 }}>{method || 'bearer'}</Tag>,
    },
    {
      title: t('providers.baseUrl'),
      dataIndex: 'base_url',
      key: 'base_url',
      width: 240,
      ellipsis: { showTitle: false },
      render: (url: string) => (
        <Tooltip title={url}>
          <Text style={{ fontSize: 12 }}>{url}</Text>
        </Tooltip>
      ),
    },
    {
      title: t('providers.group'),
      dataIndex: 'provider_group',
      key: 'provider_group',
      width: 100,
      ellipsis: { showTitle: false },
      render: (group: string) => (
        <Tooltip title={group}>
          <Tag color="blue" style={{ maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', display: 'inline-block' }}>{group}</Tag>
        </Tooltip>
      ),
    },
    {
      title: t('providers.priorityWeight'),
      key: 'priority_weight',
      width: 100,
      render: (_: any, record: Provider) => (
        <Text>{record.priority} / {record.weight}</Text>
      ),
    },
    {
      title: t('providers.active'),
      dataIndex: 'status',
      key: 'status',
      width: 70,
      render: (status: ProviderStatus, record: Provider) => (
        <Switch
          size="small"
          checked={status === ProviderStatus.Enabled}
          onChange={(checked) => handleToggleActive(record.id, checked)}
        />
      ),
    },
    {
      title: t('providers.autoBan'),
      dataIndex: 'auto_ban',
      key: 'auto_ban',
      width: 80,
      render: (ban: boolean) => (
        ban ? <Tag color="warning">{t('common.enabled')}</Tag> : <Tag>{t('common.disabled')}</Tag>
      ),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 220,
      render: (_: any, record: Provider) => (
        <Space size="small">
          <Tooltip title={t('common.edit')}>
            <Button size="small" icon={<EditOutlined />} onClick={() => openEditModal(record)} />
          </Tooltip>
          <Tooltip title={testResults[record.id]?.msg || t('providers.test')}>
            <Button
              size="small"
              loading={testingIds.has(record.id)}
              icon={
                testResults[record.id]
                  ? testResults[record.id].ok
                    ? <CheckCircleOutlined style={{ color: '#52c41a' }} />
                    : <CloseCircleOutlined style={{ color: '#f5222d' }} />
                  : isSearchProvider(record) ? <SearchOutlined /> : <ThunderboltOutlined />
              }
              onClick={() => isSearchProvider(record) ? openSearchTestModal(record) : handleTest(record.id)}
            />
          </Tooltip>
          <Tooltip title={t('providers.resetCircuitBreaker')}>
            <Button size="small" icon={<ReloadOutlined />} onClick={() => handleResetCircuitBreaker(record.id)} />
          </Tooltip>
          <Popconfirm
            title={t('providers.confirmDelete')}
            onConfirm={() => handleDelete(record.id)}
            okText={t('common.yes')}
            cancelText={t('common.no')}
          >
            <Tooltip title={t('common.delete')}>
              <Button size="small" danger icon={<DeleteOutlined />} />
            </Tooltip>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Space size="large">
          <Typography.Title level={4} style={{ margin: 0 }}>{t('providers.title')}</Typography.Title>
          <Segmented
            value={categoryFilter}
            onChange={(val) => { setCategoryFilter(val as string); setPage(1); }}
            options={[
              { value: 'all', label: t('common.all', 'All') },
              { value: 'llm', label: t('providers.categoryLLM', 'Model') },
              { value: 'search', label: t('providers.categorySearch', 'Search') },
            ]}
          />
        </Space>
        <Space>
          <Upload accept=".json" showUploadList={false} beforeUpload={handleImport}>
            <Button icon={<UploadOutlined />}>{t('providers.import')}</Button>
          </Upload>
          <Button icon={<DownloadOutlined />} onClick={handleExport}>{t('providers.export')}</Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateModal}>
            {t('providers.add')}
          </Button>
        </Space>
      </div>

      <Table
        columns={columns}
        dataSource={providers}
        rowKey="id"
        loading={loading}
        pagination={{
          current: page,
          pageSize,
          total,
          showSizeChanger: true,
          showQuickJumper: total > 50,
          showTotal: (t) => `共 ${t} 条`,
          onChange: (p, ps) => { setPage(p); setPageSize(ps); },
        }}
        scroll={{ x: 1200 }}
        size="middle"
        expandable={{
          expandedRowRender: (record) => {
            const eps = providerEndpoints[record.id];
            const formatColors: Record<string, string> = {
              openai: 'green',
              anthropic: 'orange',
              openai_responses: 'blue',
              google: 'purple',
            };
            return (
              <div style={{ padding: '8px 0' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
                  <Text strong style={{ fontSize: 13 }}>Endpoints</Text>
                  <Button
                    size="small"
                    type="dashed"
                    icon={<PlusOutlined />}
                    onClick={() => openAddEndpointModal(record.id)}
                  >
                    Add
                  </Button>
                </div>
                {!eps ? (
                  <Text type="secondary">Loading...</Text>
                ) : eps.length === 0 ? (
                  <Text type="secondary">No endpoints configured</Text>
                ) : (
                  <table style={{ borderCollapse: 'collapse', width: '100%', maxWidth: 700 }}>
                    <thead>
                      <tr style={{ borderBottom: '1px solid #f0f0f0' }}>
                        <th style={{ padding: '6px 12px', textAlign: 'left', fontSize: 12, color: '#888', fontWeight: 500 }}>Type</th>
                        <th style={{ padding: '6px 12px', textAlign: 'left', fontSize: 12, color: '#888', fontWeight: 500 }}>Format</th>
                        <th style={{ padding: '6px 12px', textAlign: 'left', fontSize: 12, color: '#888', fontWeight: 500 }}>Path</th>
                        <th style={{ padding: '6px 4px', textAlign: 'center', fontSize: 12, color: '#888', fontWeight: 500 }}></th>
                      </tr>
                    </thead>
                    <tbody>
                      {eps.map((ep, idx) => (
                        <tr key={idx} style={{ borderBottom: '1px solid #fafafa' }}>
                          <td style={{ padding: '6px 12px' }}>
                            <Text style={{ fontSize: 13 }}>{ep.endpoint_type}</Text>
                          </td>
                          <td style={{ padding: '6px 12px' }}>
                            <Tag color={formatColors[ep.api_format] || 'default'} style={{ margin: 0 }}>
                              {ep.api_format}
                            </Tag>
                          </td>
                          <td style={{ padding: '6px 12px' }}>
                            <Text code style={{ fontSize: 12 }}>{ep.endpoint_path}</Text>
                          </td>
                          <td style={{ padding: '6px 4px', textAlign: 'center' }}>
                            <Popconfirm
                              title="Delete this endpoint?"
                              onConfirm={() => handleDeleteEndpoint(record.id, (ep as any).id || 0)}
                              okText="Yes"
                              cancelText="No"
                            >
                              <Button type="text" size="small" danger style={{ fontSize: 12 }}>×</Button>
                            </Popconfirm>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            );
          },
          onExpand: (expanded, record) => {
            if (expanded && !providerEndpoints[record.id]) {
              fetchEndpoints(record.id);
            }
          },
        }}
      />

      <Modal
        title={editingProvider ? t('providers.edit') : t('providers.create')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        width={640}
        footer={
          <Space>
            <Button onClick={() => setModalOpen(false)}>{t('common.cancel')}</Button>
            {editingProvider && (
              <Button
                icon={<ThunderboltOutlined />}
                loading={modalTesting}
                onClick={handleModalTest}
              >
                {t('providers.test')}
              </Button>
            )}
            <Button type="primary" onClick={handleSubmit}>
              {editingProvider ? t('common.save') : t('common.create')}
            </Button>
          </Space>
        }
      >
        <Form form={form} layout="vertical" size="middle">
          {/* Provider Preset Selection */}
          <Form.Item
            name="preset_key"
            label={t('providers.type')}
          >
            <Select
              placeholder={editingProvider ? t('providers.typeUnchanged', 'Type not changed — select to re-apply preset') : t('providers.selectType')}
              onChange={handlePresetChange}
              showSearch
              optionFilterProp="label"
              allowClear
              options={presets.filter(p => categoryFilter === 'search' ? p.category === 'search' : p.category !== 'search').map(p => ({ value: p.preset_key, label: p.display_name || p.preset_key }))}
            />
          </Form.Item>

          {selectedPreset && (
            <div style={{ marginBottom: 16, padding: '8px 12px', background: '#f5f5f5', borderRadius: 6 }}>
              <Text type="secondary" style={{ fontSize: 12 }}>
                Auth: <strong>{selectedPreset.auth_method}</strong> | Endpoints: <strong>{selectedPreset.endpoints.length}</strong>
              </Text>
              {selectedPreset.endpoints.length > 0 && (
                <div style={{ marginTop: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>
                    {selectedPreset.endpoints.map(e => `${e.endpoint_type}(${e.format})`).join(', ')}
                  </Text>
                </div>
              )}
            </div>
          )}

          {/* Basic Fields */}
          <Form.Item name="name" label={t('providers.name')} rules={[{ required: true }]}>
            <Input placeholder="e.g. openai-main" />
          </Form.Item>

          <Form.Item name="display_name" label={t('providers.displayName')}>
            <Input />
          </Form.Item>

          <Form.Item
            name="api_key"
            label={t('providers.apiKey')}
            rules={editingProvider ? [] : [{ required: true, message: t('common.required') }]}
            extra={editingProvider && editingProvider.api_key ? (() => {
              const key = editingProvider.api_key;
              // Show prefix/suffix hint for masked key
              const display = key.startsWith('enc:')
                ? '(encrypted)'
                : key.length > 8
                  ? `${key.slice(0, 4)}...${key.slice(-4)}`
                  : '****';
              return <Text type="secondary" style={{ fontSize: 11 }}>Current: {display} — leave empty to keep unchanged</Text>;
            })() : undefined}
          >
            <Input.Password placeholder={editingProvider ? t('providers.apiKeyUnchanged', 'Leave empty to keep current key') : 'sk-...'} />
          </Form.Item>

          <Form.Item name="base_url" label={t('providers.baseUrl')} rules={[{ required: true }]}>
            <Input placeholder="https://api.openai.com/v1" />
          </Form.Item>

          <Form.Item name="auth_method" label="Auth Method">
            <Select
              options={[
                { value: 'bearer', label: 'Bearer Token' },
                { value: 'x-api-key', label: 'X-API-Key' },
                { value: 'query', label: 'Query Parameter' },
                { value: 'none', label: 'None' },
              ]}
            />
          </Form.Item>

          <Form.Item name="provider_group" label={t('providers.group')} rules={[{ required: true }]}>
            <Input placeholder="default" />
          </Form.Item>

          <Form.Item name="category" label={t('providers.category', 'Category')}>
            <Select
              options={[
                { value: 'llm', label: 'LLM' },
                { value: 'search', label: 'Search' },
              ]}
            />
          </Form.Item>

          <div style={{ display: 'flex', gap: 16 }}>
            <Form.Item name="priority" label={t('providers.priority')} style={{ flex: 1 }}>
              <InputNumber style={{ width: '100%' }} min={0} />
            </Form.Item>
            <Form.Item name="weight" label={t('providers.weight')} style={{ flex: 1 }}>
              <InputNumber style={{ width: '100%' }} min={0} />
            </Form.Item>
          </div>

          <div style={{ display: 'flex', gap: 24, marginBottom: 16 }}>
            <Form.Item name="status" label={t('providers.active')} style={{ marginBottom: 0 }}>
              <Radio.Group>
                <Radio value={ProviderStatus.Enabled}>{t('providers.statusEnabled', 'Enabled')}</Radio>
                <Radio value={ProviderStatus.ManuallyDisabled}>{t('providers.statusDisabled', 'Disabled')}</Radio>
              </Radio.Group>
            </Form.Item>
            <Form.Item name="auto_ban" label={t('providers.autoBan')} valuePropName="checked" style={{ marginBottom: 0 }}>
              <Switch />
            </Form.Item>
          </div>

          {/* Models Section */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
              <Text strong>{t('providers.models')}</Text>
              <Space size="small">
                <Button
                  size="small"
                  loading={fetchingModels}
                  onClick={handleFetchUpstreamModels}
                >
                  {t('providers.fetchModels')}
                </Button>
              </Space>
            </div>
            <Form.Item name="model_patterns" style={{ marginBottom: 0 }}>
              <TextArea rows={4} placeholder="gpt-4o&#10;gpt-4o-mini&#10;claude-sonnet-4-20250514" />
            </Form.Item>
          </div>

          {/* Advanced Section */}
          <Collapse
            ghost
            items={[{
              key: 'advanced',
              label: t('providers.advancedSettings'),
              children: (
                <>
                  <div style={{ display: 'flex', gap: 16 }}>
                    <Form.Item name="rpm_limit" label={t('providers.rpmLimit')} style={{ flex: 1 }}>
                      <InputNumber style={{ width: '100%' }} min={-1} placeholder={t('providers.rpmPlaceholder')} />
                    </Form.Item>
                    <Form.Item name="tpm_limit" label={t('providers.tpmLimit')} style={{ flex: 1 }}>
                      <InputNumber style={{ width: '100%' }} min={-1} placeholder={t('providers.tpmPlaceholder')} />
                    </Form.Item>
                    <Form.Item name="concurrent_limit" label={t('providers.concurrentLimit')} style={{ flex: 1 }}>
                      <InputNumber style={{ width: '100%' }} min={-1} placeholder={t('providers.concurrentPlaceholder')} />
                    </Form.Item>
                  </div>

                  <Form.Item label={t('providers.customHeaders')} style={{ marginBottom: 12 }}>
                    <Form.List name="custom_headers_list">
                      {(fields, { add, remove }) => (
                        <>
                          {fields.map(({ key, name, ...restField }) => (
                            <div key={key} style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                              <Form.Item {...restField} name={[name, 'key']} style={{ flex: 1, marginBottom: 0 }}
                                rules={[{ required: true, message: 'Header name' }]}>
                                <Input placeholder="Header-Name" size="small" />
                              </Form.Item>
                              <Form.Item {...restField} name={[name, 'value']} style={{ flex: 2, marginBottom: 0 }}
                                rules={[{ required: true, message: 'Value' }]}>
                                <Input placeholder="value" size="small" />
                              </Form.Item>
                              <MinusCircleOutlined onClick={() => remove(name)} style={{ marginTop: 8, color: '#999' }} />
                            </div>
                          ))}
                          <Button type="dashed" onClick={() => add()} block size="small" icon={<PlusOutlined />}>
                            {t('providers.addHeader') || 'Add Header'}
                          </Button>
                        </>
                      )}
                    </Form.List>
                  </Form.Item>

                  <Form.Item name="remark" label={t('providers.remark')}>
                    <TextArea rows={2} placeholder={t('providers.remarkPlaceholder')} />
                  </Form.Item>

                  {/* Inline Endpoints configuration — available for all providers */}
                  <div style={{ borderTop: '1px solid #f0f0f0', paddingTop: 12, marginTop: 12 }}>
                    <Text type="secondary" style={{ display: 'block', marginBottom: 8 }}>
                      Endpoints{selectedPreset ? ' (pre-filled from preset, editable)' : ' (optional — auto-inferred from Auth Method if not configured)'}
                    </Text>
                    <Form.List name="endpoints_list">
                      {(fields, { add, remove }) => (
                        <>
                          {fields.map(({ key, name, ...restField }) => (
                            <div key={key} style={{ display: 'flex', gap: 8, marginBottom: 8, alignItems: 'center' }}>
                              <Form.Item {...restField} name={[name, 'endpoint_type']} style={{ flex: 1, marginBottom: 0 }}
                                rules={[{ required: true, message: 'Type' }]}>
                                <Select size="small" placeholder="Type" onChange={(val) => {
                                  const eps = form.getFieldValue('endpoints_list') || [];
                                  const fmt = eps[name]?.api_format || 'openai';
                                  const pathMap: Record<string, Record<string, string>> = {
                                    chat: { openai: '/v1/chat/completions', anthropic: '/v1/messages', gemini: '/v1/generateContent', openai_responses: '/v1/responses' },
                                    responses: { openai: '/v1/responses', openai_responses: '/v1/responses' },
                                    embeddings: { openai: '/v1/embeddings' },
                                    images: { openai: '/v1/images/generations' },
                                    images_edits: { openai: '/v1/images/edits' },
                                    images_variations: { openai: '/v1/images/variations' },
                                    audio_speech: { openai: '/v1/audio/speech' },
                                    audio_transcriptions: { openai: '/v1/audio/transcriptions' },
                                    audio_translations: { openai: '/v1/audio/translations' },
                                    completions: { openai: '/v1/completions' },
                                    rerank: { openai: '/v1/rerank' },
                                  };
                                  const path = pathMap[val]?.[fmt];
                                  if (eps[name]) {
                                    if (val === 'custom') {
                                      eps[name].endpoint_path = '';
                                    } else if (path) {
                                      eps[name].endpoint_path = path;
                                    }
                                    form.setFieldsValue({ endpoints_list: [...eps] });
                                  }
                                }} options={[
                                  { value: 'chat', label: 'Chat' },
                                  { value: 'responses', label: 'Responses' },
                                  { value: 'completions', label: 'Completions' },
                                  { value: 'embeddings', label: 'Embeddings' },
                                  { value: 'images', label: 'Images (Generations)' },
                                  { value: 'images_edits', label: 'Images (Edits)' },
                                  { value: 'images_variations', label: 'Images (Variations)' },
                                  { value: 'audio_speech', label: 'Audio Speech' },
                                  { value: 'audio_transcriptions', label: 'Audio Transcriptions' },
                                  { value: 'audio_translations', label: 'Audio Translations' },
                                  { value: 'rerank', label: 'Rerank' },
                                  { value: 'custom', label: 'Custom' },
                                ]} />
                              </Form.Item>
                              <Form.Item {...restField} name={[name, 'api_format']} style={{ flex: 1, marginBottom: 0 }}
                                rules={[{ required: true, message: 'Format' }]}>
                                <Select size="small" placeholder="Format" onChange={(val) => {
                                  const eps = form.getFieldValue('endpoints_list') || [];
                                  const epType = eps[name]?.endpoint_type || 'chat';
                                  const pathMap: Record<string, Record<string, string>> = {
                                    chat: { openai: '/v1/chat/completions', anthropic: '/v1/messages', gemini: '/v1/generateContent', openai_responses: '/v1/responses' },
                                    responses: { openai: '/v1/responses', openai_responses: '/v1/responses' },
                                    embeddings: { openai: '/v1/embeddings' },
                                    images: { openai: '/v1/images/generations' },
                                    images_edits: { openai: '/v1/images/edits' },
                                    images_variations: { openai: '/v1/images/variations' },
                                    audio_speech: { openai: '/v1/audio/speech' },
                                    audio_transcriptions: { openai: '/v1/audio/transcriptions' },
                                    audio_translations: { openai: '/v1/audio/translations' },
                                    completions: { openai: '/v1/completions' },
                                    rerank: { openai: '/v1/rerank' },
                                  };
                                  const path = pathMap[epType]?.[val] || pathMap['chat']?.[val];
                                  if (path && eps[name]) {
                                    eps[name].endpoint_path = path;
                                    form.setFieldsValue({ endpoints_list: [...eps] });
                                  }
                                }} options={[
                                  { value: 'openai', label: 'OpenAI' },
                                  { value: 'anthropic', label: 'Anthropic' },
                                  { value: 'openai_responses', label: 'OpenAI Responses' },
                                  { value: 'gemini', label: 'Gemini' },
                                  { value: 'passthrough', label: 'Passthrough' },
                                ]} />
                              </Form.Item>
                              <Form.Item {...restField} name={[name, 'endpoint_path']} style={{ flex: 2, marginBottom: 0 }}
                                rules={[{ required: true, message: 'Path' }]}>
                                <Input size="small" placeholder="/v1/chat/completions" />
                              </Form.Item>
                              <MinusCircleOutlined onClick={() => remove(name)} style={{ color: '#999' }} />
                            </div>
                          ))}
                          <Button type="dashed" onClick={() => add({ endpoint_type: 'chat', api_format: 'openai', endpoint_path: '/v1/chat/completions' })} block size="small" icon={<PlusOutlined />}>
                            Add Endpoint
                          </Button>
                        </>
                      )}
                    </Form.List>
                  </div>
                </>
              ),
            }]}
          />
        </Form>
      </Modal>

      {/* Add Endpoint Modal */}
      <Modal
        title="Add Endpoint"
        open={addEpModalOpen}
        onCancel={() => setAddEpModalOpen(false)}
        onOk={handleAddEpSubmit}
        okText="Add"
        width={420}
      >
        <Form form={addEpForm} layout="vertical" size="middle">
          <Form.Item name="endpoint_type" label="Endpoint Type" rules={[{ required: true }]}>
            <Select options={[
              { value: 'chat', label: 'Chat' },
              { value: 'responses', label: 'Responses' },
              { value: 'completions', label: 'Completions' },
              { value: 'embeddings', label: 'Embeddings' },
              { value: 'images', label: 'Images (Generations)' },
              { value: 'images_edits', label: 'Images (Edits)' },
              { value: 'images_variations', label: 'Images (Variations)' },
              { value: 'audio_speech', label: 'Audio Speech' },
              { value: 'audio_transcriptions', label: 'Audio Transcriptions' },
              { value: 'audio_translations', label: 'Audio Translations' },
              { value: 'moderations', label: 'Moderations' },
              { value: 'rerank', label: 'Rerank' },
              { value: 'custom', label: 'Custom' },
            ]} />
          </Form.Item>
          <Form.Item name="api_format" label="Format" rules={[{ required: true }]}>
            <Select options={[
              { value: 'openai', label: 'OpenAI (/v1/chat/completions)' },
              { value: 'anthropic', label: 'Anthropic (/v1/messages)' },
              { value: 'openai_responses', label: 'OpenAI Responses (/v1/responses)' },
              { value: 'gemini', label: 'Gemini (generateContent)' },
              { value: 'passthrough', label: 'Passthrough (no conversion)' },
            ]} />
          </Form.Item>
          <Form.Item name="endpoint_path" label="Path" rules={[{ required: true }]}>
            <Input placeholder="/v1/chat/completions" />
          </Form.Item>
        </Form>
      </Modal>

      {/* Search Test Modal */}
      <Modal
        title={`Test Search — ${searchTestProvider?.name || ''}`}
        open={searchTestModalOpen}
        onCancel={() => setSearchTestModalOpen(false)}
        footer={null}
        width={700}
      >
        <Form form={searchTestForm} layout="vertical">
          <Form.Item label="Request Body (JSON)" name="body" rules={[{ required: true }]}>
            <Input.TextArea rows={6} style={{ fontFamily: 'monospace', fontSize: 12 }} />
          </Form.Item>
          <Button type="primary" loading={searchTestLoading} onClick={handleSearchTest} icon={<SearchOutlined />}>
            Send Test
          </Button>
        </Form>

        {searchTestResult && (
          <div style={{ marginTop: 16, background: '#f5f5f5', padding: 12, borderRadius: 4 }}>
            <div style={{ marginBottom: 8 }}>
              <Tag color={searchTestResult.status < 400 ? 'success' : 'error'}>
                Status: {searchTestResult.status}
              </Tag>
              {searchTestResult.provider && <Tag>{searchTestResult.provider}</Tag>}
            </div>
            <pre style={{ maxHeight: 400, overflow: 'auto', fontSize: 12, margin: 0 }}>
              {JSON.stringify(searchTestResult.response || searchTestResult, null, 2)}
            </pre>
          </div>
        )}
      </Modal>
    </div>
  );
}
