import { useEffect, useState } from 'react'
import {
  Table, Button, Space, Modal, Form, Input, InputNumber, Select, Switch, Tag, Popconfirm, Typography, DatePicker, App, Row, Col,
} from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import dayjs from 'dayjs'
import { listKeys, createKey, updateKey, deleteKey } from '@/api/keys'
import { listUsers } from '@/api/users'
import { getQuotaGroups } from '@/api/quota'
import { listProviders } from '@/api/providers'
import { listAvailableModels } from '@/api/models'

const { Paragraph } = Typography

interface ApiKey {
  id: number
  name: string
  key_prefix: string
  key_suffix: string
  full_key: string | null
  user_id: number
  user_name: string
  provider_group: string | null
  quota_limit: number
  quota_group: string | null
  used_tokens: number
  rpm_limit: number | null
  tpm_limit: number | null
  model_limits: string[] | null
  is_active: boolean
  expires_at: string | null
  created_at: string
}

interface UserOption {
  id: number
  username: string
}

export default function Keys() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [data, setData] = useState<ApiKey[]>([])
  const [users, setUsers] = useState<UserOption[]>([])
  const [quotaGroups, setQuotaGroups] = useState<string[]>([])
  const [providerGroups, setProviderGroups] = useState<string[]>([])
  const [availableModels, setAvailableModels] = useState<{ model: string; provider_group: string }[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [editingKey, setEditingKey] = useState<ApiKey | null>(null)
  const [createdKeyValue, setCreatedKeyValue] = useState<string | null>(null)
  const [form] = Form.useForm()

  const fetchData = async () => {
    setLoading(true)
    try {
      const res = await listKeys()
      setData(res.data ?? [])
    } finally {
      setLoading(false)
    }
  }

  const fetchUsers = async () => {
    try {
      const res = await listUsers()
      setUsers((res.data ?? []).map((u: any) => ({ id: u.id, username: u.username })))
    } catch {
      // ignore
    }
  }

  const fetchQuotaGroups = async () => {
    try {
      const res = await getQuotaGroups()
      setQuotaGroups(res.data?.data || res.data || [])
    } catch {
      // ignore
    }
  }

  const fetchProviderGroups = async () => {
    try {
      const res = await listProviders(1, 1000)
      const providers = res.data?.elements ?? res.data?.data ?? res.data ?? []
      const groups = [...new Set(providers.map((p: any) => p.provider_group).filter(Boolean))] as string[]
      setProviderGroups(groups)
    } catch {
      // ignore
    }
  }

  const fetchModelOptions = async () => {
    try {
      const res = await listAvailableModels()
      setAvailableModels(res.data?.data ?? res.data ?? [])
    } catch {
      // ignore
    }
  }

  // Compute model options based on selected provider_group
  const selectedGroup = Form.useWatch('provider_group', form)
  const modelOptions = (() => {
    const filtered = selectedGroup
      ? availableModels.filter(m => m.provider_group === selectedGroup)
      : availableModels
    return [...new Set(filtered.map(m => m.model))].sort()
  })()

  useEffect(() => {
    fetchData()
    fetchUsers()
    fetchQuotaGroups()
    fetchProviderGroups()
    fetchModelOptions()
  }, [])

  const handleCreate = () => {
    setEditingKey(null)
    setModalOpen(true)
  }

  const handleEdit = (record: ApiKey) => {
    setEditingKey(record)
    setModalOpen(true)
  }

  const handleDelete = async (id: number) => {
    try {
      await deleteKey(id)
      message.success(t('keys.deleted'))
      fetchData()
    } catch {
      message.error(t('keys.deleteFailed'))
    }
  }
  const handleSubmit = async () => {
    try {
      const values = await form.validateFields()
      const payload: any = { ...values }
      if (values.expires_at) {
        payload.expires_at = values.expires_at.toISOString()
      }
      // Ensure cleared fields are sent as null (not omitted)
      if (payload.quota_group === undefined || payload.quota_group === '') {
        payload.quota_group = null
      }
      if (payload.provider_group === undefined) {
        payload.provider_group = ''
      }
      // Convert empty model_limits to null
      if (!payload.model_limits || payload.model_limits.length === 0) {
        payload.model_limits = null
      }
      // null/undefined → -1 (unlimited) for backend
      if (payload.rpm_limit == null) payload.rpm_limit = -1
      if (payload.tpm_limit == null) payload.tpm_limit = -1

      if (editingKey) {
        await updateKey(editingKey.id, payload)
        message.success(t('keys.updated'))
        setModalOpen(false)
      } else {
        const res = await createKey(payload)
        message.success(t('keys.created'))
        setModalOpen(false)
        if (res.data?.key) {
          setCreatedKeyValue(res.data.key)
        }
      }
      fetchData()
    } catch {
      message.error(t('keys.formError'))
    }
  }

  const columns: ColumnsType<ApiKey> = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 50 },
    { title: t('keys.name'), dataIndex: 'name', key: 'name', width: 120, ellipsis: true },
    {
      title: 'Key', dataIndex: 'key_prefix', key: 'key_prefix', width: 200,
      render: (_: string, record: ApiKey) => {
        const masked = `${record.key_prefix}...${record.key_suffix || '****'}`
        const copyText = record.full_key || masked
        return (
          <Paragraph copyable={{ text: copyText }} style={{ marginBottom: 0, fontFamily: 'monospace', fontSize: 13 }}>
            {masked}
          </Paragraph>
        )
      },
    },
    { title: t('keys.user'), dataIndex: 'user_name', key: 'user_name', width: 80, render: (v) => v || '-' },
    {
      title: t('keys.quotaGroup'),
      dataIndex: 'quota_group',
      key: 'quota_group',
      width: 120,
      render: (v) => v ? <Tag color="blue">{v}</Tag> : <Tag>{t('common.unlimited')}</Tag>,
    },
    {
      title: 'RPM',
      dataIndex: 'rpm_limit',
      key: 'rpm_limit',
      width: 70,
      render: (v) => (v != null && v >= 0 ? v.toLocaleString() : '-'),
    },
    {
      title: 'TPM',
      dataIndex: 'tpm_limit',
      key: 'tpm_limit',
      width: 70,
      render: (v) => (v != null && v >= 0 ? v.toLocaleString() : '-'),
    },
    {
      title: t('keys.modelLimits'),
      dataIndex: 'model_limits',
      key: 'model_limits',
      width: 180,
      ellipsis: true,
      render: (v: string[] | null) => {
        if (!v || v.length === 0) return <Tag>{t('common.unlimited')}</Tag>
        return <>{v.map((m) => <Tag key={m} color="geekblue" style={{ marginBottom: 2 }}>{m}</Tag>)}</>
      },
    },
    {
      title: t('keys.active'),
      dataIndex: 'is_active',
      key: 'is_active',
      width: 60,
      render: (v) => <Tag color={v ? 'green' : 'default'}>{v ? t('common.yes') : t('common.no')}</Tag>,
    },
    {
      title: t('keys.expiresAt'),
      dataIndex: 'expires_at',
      key: 'expires_at',
      width: 100,
      render: (v) => (v ? dayjs(v).format('YYYY-MM-DD') : t('common.never')),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 120,
      fixed: 'right',
      render: (_, record) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => handleEdit(record)}>
            {t('common.edit')}
          </Button>
          <Popconfirm
            title={t('keys.deleteConfirm')}
            onConfirm={() => handleDelete(record.id)}
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
      <Space style={{ marginBottom: 16 }}>
        <Button type="primary" onClick={handleCreate}>
          {t('keys.add')}
        </Button>
      </Space>

      <Table
        columns={columns}
        dataSource={data}
        rowKey="id"
        loading={loading}
        pagination={false}
        scroll={{ x: 1000 }}
      />

      <Modal
        title={editingKey ? t('keys.edit') : t('keys.create')}
        open={modalOpen}
        onOk={handleSubmit}
        onCancel={() => setModalOpen(false)}
        width={640}
        afterOpenChange={(open) => {
          if (open) {
            form.resetFields()
            if (editingKey) {
              form.setFieldsValue({
                name: editingKey.name,
                provider_group: editingKey.provider_group,
                quota_group: editingKey.quota_group,
                rpm_limit: editingKey.rpm_limit != null && editingKey.rpm_limit >= 0 ? editingKey.rpm_limit : null,
                tpm_limit: editingKey.tpm_limit != null && editingKey.tpm_limit >= 0 ? editingKey.tpm_limit : null,
                model_limits: editingKey.model_limits || [],
                is_active: editingKey.is_active,
              })
            } else {
              form.setFieldsValue({ is_active: true })
            }
          }
        }}
        destroyOnHidden
      >
        <Form form={form} layout="vertical">
          <Row gutter={16}>
            <Col span={12}>
              <Form.Item
                name="name"
                label={t('keys.name')}
                rules={[{ required: true, message: t('keys.nameRequired') }]}
              >
                <Input placeholder={t('keys.namePlaceholder')} />
              </Form.Item>
            </Col>
            <Col span={12}>
              {!editingKey ? (
                <Form.Item
                  name="user_id"
                  label={t('keys.user')}
                  rules={[{ required: true, message: t('common.required') }]}
                >
                  <Select placeholder={t('keys.selectUser')}>
                    {users.map((u) => (
                      <Select.Option key={u.id} value={u.id}>
                        {u.username}
                      </Select.Option>
                    ))}
                  </Select>
                </Form.Item>
              ) : (
                <Form.Item label={t('keys.active')} name="is_active" valuePropName="checked">
                  <Switch />
                </Form.Item>
              )}
            </Col>
          </Row>
          <Row gutter={16}>
            <Col span={12}>
              <Form.Item name="provider_group" label={t('keys.providerGroup')}>
                <Select
                  placeholder={t('keys.providerGroupPlaceholder')}
                  allowClear
                  showSearch
                  options={providerGroups.map(g => ({ value: g, label: g }))}
                  onChange={() => form.setFieldValue('model_limits', [])}
                />
              </Form.Item>
            </Col>
            <Col span={12}>
              <Form.Item name="quota_group" label={t('keys.quotaGroup')}>
                <Select
                  placeholder={t('keys.quotaGroupPlaceholder')}
                  allowClear
                  showSearch
                  options={quotaGroups.map(g => ({ value: g, label: g }))}
                />
              </Form.Item>
            </Col>
          </Row>
          <Form.Item name="model_limits" label={t('keys.modelLimits')} extra={t('keys.modelLimitsHint')}>
            <Select
              mode="multiple"
              placeholder={t('keys.modelLimitsPlaceholder')}
              style={{ width: '100%' }}
              allowClear
              showSearch
              options={modelOptions.map(m => ({ value: m, label: m }))}
            />
          </Form.Item>
          <Row gutter={16}>
            <Col span={12}>
              <Form.Item name="rpm_limit" label={t('keys.rpmLimit')}>
                <InputNumber min={0} style={{ width: '100%' }} placeholder={t('keys.rpmPlaceholder')} controls={false}
                  parser={(v) => { const n = Number(v); return (v === '' || v == null) ? (null as any) : (Number.isNaN(n) ? (null as any) : n); }}
                />
              </Form.Item>
            </Col>
            <Col span={12}>
              <Form.Item name="tpm_limit" label={t('keys.tpmLimit')}>
                <InputNumber min={0} style={{ width: '100%' }} placeholder={t('keys.tpmPlaceholder')} controls={false}
                  parser={(v) => { const n = Number(v); return (v === '' || v == null) ? (null as any) : (Number.isNaN(n) ? (null as any) : n); }}
                />
              </Form.Item>
            </Col>
          </Row>
          <Row gutter={16}>
            {!editingKey && (
              <Col span={12}>
                <Form.Item name="expires_at" label={t('keys.expiresAt')}>
                  <DatePicker style={{ width: '100%' }} />
                </Form.Item>
              </Col>
            )}
            {!editingKey && (
              <Col span={12}>
                <Form.Item label={t('keys.active')} name="is_active" valuePropName="checked">
                  <Switch />
                </Form.Item>
              </Col>
            )}
          </Row>
        </Form>
      </Modal>

      <Modal
        title={t('keys.createdTitle')}
        open={!!createdKeyValue}
        onOk={() => setCreatedKeyValue(null)}
        onCancel={() => setCreatedKeyValue(null)}
        cancelButtonProps={{ style: { display: 'none' } }}
      >
        <p style={{ marginBottom: 8, color: '#ff4d4f', fontWeight: 500 }}>
          {t('keys.createdWarning')}
        </p>
        <Paragraph copyable={{ text: createdKeyValue ?? '' }} style={{ fontFamily: 'monospace' }}>
          {createdKeyValue}
        </Paragraph>
      </Modal>
    </div>
  )
}
