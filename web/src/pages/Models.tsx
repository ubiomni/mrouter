import { useEffect, useState } from 'react'
import {
  Table, Button, Space, Tabs, Modal, Form, Input, InputNumber, Select, Switch, Tag, Popconfirm, App,
} from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import {
  listProviderModels,
  createProviderModel,
  updateProviderModel,
  deleteProviderModel,
  listModelsMetadata,
  upsertModelMetadata,
  deleteModelMetadata,
} from '@/api/models'
import { listProviders, type Provider } from '@/api/providers'

interface ProviderModel {
  id: number
  provider_id: number
  provider_name: string
  model_pattern: string
  upstream_model: string | null
  priority: number
  weight: number
  enabled: boolean
}

interface ModelMetadata {
  model_name: string
  display_name: string | null
  vendor: string | null
  tags: string[]
  capabilities: string[]
  is_visible: boolean
}

export default function Models() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [providers, setProviders] = useState<Provider[]>([])
  const [providerModels, setProviderModels] = useState<ProviderModel[]>([])
  const [metadata, setMetadata] = useState<ModelMetadata[]>([])
  const [loadingPM, setLoadingPM] = useState(false)
  const [loadingMeta, setLoadingMeta] = useState(false)
  const [selectedProvider, setSelectedProvider] = useState<number | undefined>(undefined)

  const [pmModalOpen, setPmModalOpen] = useState(false)
  const [editingPM, setEditingPM] = useState<ProviderModel | null>(null)
  const [pmForm] = Form.useForm()

  const [metaModalOpen, setMetaModalOpen] = useState(false)
  const [editingMeta, setEditingMeta] = useState<ModelMetadata | null>(null)
  const [metaForm] = Form.useForm()
  const fetchProviders = async () => {
    try {
      const res = await listProviders(1, 200, 'llm')
      setProviders(res.data?.elements ?? [])
    } catch {
      // ignore
    }
  }

  const fetchProviderModels = async (providerId?: number) => {
    if (!providerId) {
      setProviderModels([])
      return
    }
    setLoadingPM(true)
    try {
      const res = await listProviderModels(providerId)
      setProviderModels(res.data ?? [])
    } finally {
      setLoadingPM(false)
    }
  }

  const fetchMetadata = async () => {
    setLoadingMeta(true)
    try {
      const res = await listModelsMetadata()
      setMetadata(res.data ?? [])
    } finally {
      setLoadingMeta(false)
    }
  }

  useEffect(() => {
    fetchProviders()
    fetchMetadata()
  }, [])

  useEffect(() => {
    fetchProviderModels(selectedProvider)
  }, [selectedProvider])

  // Initialize metadata form when modal opens
  useEffect(() => {
    if (metaModalOpen) {
      if (editingMeta) {
        metaForm.setFieldsValue({
          model_name: editingMeta.model_name,
          display_name: editingMeta.display_name,
          vendor: editingMeta.vendor,
          tags: editingMeta.tags,
          capabilities: editingMeta.capabilities || [],
          is_visible: editingMeta.is_visible,
        })
      } else {
        metaForm.resetFields()
        metaForm.setFieldsValue({ is_visible: true, tags: [], capabilities: [] })
      }
    }
  }, [metaModalOpen, editingMeta, metaForm])

  // Initialize provider model form when modal opens
  useEffect(() => {
    if (pmModalOpen) {
      if (editingPM) {
        pmForm.setFieldsValue({
          model_pattern: editingPM.model_pattern,
          upstream_model: editingPM.upstream_model,
          priority: editingPM.priority,
          weight: editingPM.weight,
          enabled: editingPM.enabled,
        })
      } else {
        pmForm.resetFields()
        pmForm.setFieldsValue({ priority: 0, weight: 1, enabled: true })
      }
    }
  }, [pmModalOpen, editingPM, pmForm])

  // Provider Models handlers
  const handleCreatePM = () => {
    if (!selectedProvider) {
      message.warning(t('models.selectProviderFirst'))
      return
    }
    setEditingPM(null)
    setPmModalOpen(true)
  }

  const handleEditPM = (record: ProviderModel) => {
    setEditingPM(record)
    setPmModalOpen(true)
  }

  const handleDeletePM = async (id: number) => {
    try {
      await deleteProviderModel(id)
      message.success(t('models.deleted'))
      fetchProviderModels(selectedProvider)
    } catch {
      message.error(t('models.deleteFailed'))
    }
  }
  const handleSubmitPM = async () => {
    try {
      const values = await pmForm.validateFields()
      if (editingPM) {
        await updateProviderModel(editingPM.id, values)
        message.success(t('models.updated'))
      } else {
        await createProviderModel(selectedProvider!, values)
        message.success(t('models.created'))
      }
      setPmModalOpen(false)
      fetchProviderModels(selectedProvider)
    } catch {
      message.error(t('models.formError'))
    }
  }

  // Metadata handlers
  const handleCreateMeta = () => {
    setEditingMeta(null)
    setMetaModalOpen(true)
  }

  const handleEditMeta = (record: ModelMetadata) => {
    setEditingMeta(record)
    setMetaModalOpen(true)
  }

  // Open metadata modal for a provider model (create or edit)
  const handleManageMetaForModel = async (modelPattern: string) => {
    // Check if metadata already exists
    const existing = metadata.find(m => m.model_name === modelPattern)
    if (existing) {
      setEditingMeta(existing)
    } else {
      // Pre-fill model_name for new metadata
      setEditingMeta({
        model_name: modelPattern,
        display_name: null,
        vendor: null,
        tags: [],
        capabilities: [],
        is_visible: true
      })
    }
    setMetaModalOpen(true)
  }

  const handleDeleteMeta = async (modelName: string) => {
    try {
      await deleteModelMetadata(modelName)
      message.success(t('models.metaDeleted'))
      fetchMetadata()
    } catch {
      message.error(t('models.metaDeleteFailed'))
    }
  }

  const handleSubmitMeta = async () => {
    try {
      const values = await metaForm.validateFields()
      // Convert tags array to comma-separated string, or null if empty
      const payload = {
        ...values,
        tags: values.tags?.length ? values.tags.join(',') : null,
        display_name: values.display_name || null,
        vendor: values.vendor || null,
      }
      await upsertModelMetadata(payload)
      message.success(t('models.metaSaved'))
      setMetaModalOpen(false)
      fetchMetadata()
    } catch {
      message.error(t('models.formError'))
    }
  }
  const pmColumns: ColumnsType<ProviderModel> = [
    { title: t('common.id'), dataIndex: 'id', key: 'id', width: 60 },
    { title: t('models.modelPattern'), dataIndex: 'model_pattern', key: 'model_pattern' },
    { title: t('models.upstreamModel'), dataIndex: 'upstream_model', key: 'upstream_model', render: (v) => v || t('models.same') },
    { title: t('models.provider'), dataIndex: 'provider_name', key: 'provider_name' },
    { title: t('models.priority'), dataIndex: 'priority', key: 'priority', width: 80 },
    { title: t('models.weight'), dataIndex: 'weight', key: 'weight', width: 80 },
    {
      title: t('models.enabled'),
      dataIndex: 'enabled',
      key: 'enabled',
      width: 80,
      render: (v) => <Tag color={v ? 'green' : 'default'}>{v ? t('common.yes') : t('common.no')}</Tag>,
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 200,
      render: (_, record) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => handleEditPM(record)}>
            {t('common.edit')}
          </Button>
          <Button type="link" size="small" onClick={() => handleManageMetaForModel(record.model_pattern)}>
            {t('models.metadata', '元数据')}
          </Button>
          <Popconfirm
            title={t('models.deleteConfirm')}
            onConfirm={() => handleDeletePM(record.id)}
            okText={t('common.yes')}
            cancelText={t('common.no')}
          >
            <Button type="link" size="small" danger>
              {t('common.delete')}
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  const metaColumns: ColumnsType<ModelMetadata> = [
    { title: t('models.modelName'), dataIndex: 'model_name', key: 'model_name' },
    { title: t('models.displayName'), dataIndex: 'display_name', key: 'display_name', render: (v) => v || '-' },
    { title: t('models.vendor'), dataIndex: 'vendor', key: 'vendor', render: (v) => v || '-' },
    {
      title: t('models.tags'),
      dataIndex: 'tags',
      key: 'tags',
      render: (tags: string[]) =>
        tags?.length ? tags.map((tag) => <Tag key={tag}>{tag}</Tag>) : '-',
    },
    {
      title: t('models.capabilities', 'Capabilities'),
      dataIndex: 'capabilities',
      key: 'capabilities',
      render: (caps: string[]) =>
        caps?.length ? caps.map((cap) => <Tag key={cap} color="blue">{cap}</Tag>) : '-',
    },
    {
      title: t('models.visible'),
      dataIndex: 'is_visible',
      key: 'is_visible',
      width: 80,
      render: (v) => <Tag color={v ? 'green' : 'default'}>{v ? t('common.yes') : t('common.no')}</Tag>,
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 150,
      render: (_, record) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => handleEditMeta(record)}>
            {t('common.edit')}
          </Button>
          <Popconfirm
            title={t('models.deleteMetaConfirm')}
            onConfirm={() => handleDeleteMeta(record.model_name)}
            okText={t('common.yes')}
            cancelText={t('common.no')}
          >
            <Button type="link" size="small" danger>
              {t('common.delete')}
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]
  return (
    <div style={{ padding: 12 }}>
      <Tabs
        defaultActiveKey="provider-models"
        items={[
          {
            key: 'provider-models',
            label: t('models.providerModels'),
            children: (
              <>
                <Space style={{ marginBottom: 16 }}>
                  <Select
                    style={{ width: 240 }}
                    placeholder={t('models.selectProvider')}
                    allowClear
                    value={selectedProvider}
                    onChange={(v) => setSelectedProvider(v)}
                  >
                    {providers.map((p) => (
                      <Select.Option key={p.id} value={p.id}>
                        {p.name}
                      </Select.Option>
                    ))}
                  </Select>
                  <Button type="primary" onClick={handleCreatePM}>
                    {t('models.addModel')}
                  </Button>
                </Space>
                <Table
                  columns={pmColumns}
                  dataSource={providerModels}
                  rowKey="id"
                  loading={loadingPM}
                  pagination={false}
                />
              </>
            ),
          },
          {
            key: 'metadata',
            label: t('models.metadata'),
            children: (
              <>
                <Space style={{ marginBottom: 16 }}>
                  <Button type="primary" onClick={handleCreateMeta}>
                    {t('models.addMetadata')}
                  </Button>
                </Space>
                <Table
                  columns={metaColumns}
                  dataSource={metadata}
                  rowKey="model_name"
                  loading={loadingMeta}
                  pagination={false}
                />
              </>
            ),
          },
        ]}
      />

      {/* Provider Model Modal */}
      <Modal
        title={editingPM ? t('models.editProviderModel') : t('models.addProviderModel')}
        open={pmModalOpen}
        onOk={handleSubmitPM}
        onCancel={() => setPmModalOpen(false)}
        destroyOnHidden
      >
        <Form form={pmForm} layout="vertical">
          <Form.Item
            name="model_pattern"
            label={t('models.modelPattern')}
            rules={[{ required: true, message: t('models.modelPatternRequired') }]}
          >
            <Input placeholder={t('models.modelPatternPlaceholder')} />
          </Form.Item>
          <Form.Item name="upstream_model" label={t('models.upstreamModel')}>
            <Input placeholder={t('models.upstreamModelPlaceholder')} />
          </Form.Item>
          <Form.Item name="priority" label={t('models.priority')}>
            <InputNumber min={0} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="weight" label={t('models.weight')}>
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="enabled" label={t('models.enabled')} valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </Modal>

      {/* Model Metadata Modal */}
      <Modal
        title={editingMeta ? t('models.editModelMetadata') : t('models.addModelMetadata')}
        open={metaModalOpen}
        onOk={handleSubmitMeta}
        onCancel={() => setMetaModalOpen(false)}
        destroyOnHidden
      >
        <Form form={metaForm} layout="vertical">
          <Form.Item
            name="model_name"
            label={t('models.modelName')}
            rules={[{ required: true, message: t('models.modelNameRequired') }]}
          >
            <Input disabled={!!editingMeta} placeholder={t('models.modelNamePlaceholder')} />
          </Form.Item>
          <Form.Item name="display_name" label={t('models.displayName')}>
            <Input placeholder={t('models.displayNamePlaceholder')} />
          </Form.Item>
          <Form.Item name="vendor" label={t('models.vendor')}>
            <Input placeholder={t('models.vendorPlaceholder')} />
          </Form.Item>
          <Form.Item name="tags" label={t('models.tags')}>
            <Select mode="tags" placeholder={t('models.tagsPlaceholder')} />
          </Form.Item>
          <Form.Item name="capabilities" label={t('models.capabilities', 'Capabilities')}>
            <Select
              mode="multiple"
              placeholder="Select capabilities"
              options={[
                { value: 'chat', label: 'Chat' },
                { value: 'completion', label: 'Completion' },
                { value: 'embedding', label: 'Embedding' },
                { value: 'image', label: 'Image Generation' },
                { value: 'image_edit', label: 'Image Edit' },
                { value: 'video', label: 'Video Generation' },
                { value: 'video_edit', label: 'Video Edit' },
                { value: 'vision', label: 'Vision' },
                { value: 'asr', label: 'ASR (Speech Recognition)' },
                { value: 'tts', label: 'TTS (Text-to-Speech)' },
                { value: 'rerank', label: 'Rerank' },
                { value: 'function_calling', label: 'Function Calling' },
                { value: 'thinking', label: 'Thinking' },
                { value: 'audio', label: 'Audio Processing' },
              ]}
            />
          </Form.Item>
          <Form.Item name="is_visible" label={t('models.visible')} valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
