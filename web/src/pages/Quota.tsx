import { useEffect, useState } from 'react'
import {
  Card, Table, Button, Tabs, Tag, Space, Modal, Form, Input, InputNumber, Select, Popconfirm, App, Descriptions, Empty,
} from 'antd'
import { PlusOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '@/stores/auth'
import { listKeys } from '@/api/keys'
import {
  listQuotaRules, createQuotaRule, updateQuotaRule, deleteQuotaRule,
  getQuotaUsage, getQuotaGroups, invalidateQuotaCache,
} from '@/api/quota'

interface QuotaRule {
  id: number
  name: string
  quota_group: string | null
  dimension: string
  model_pattern: string
  limit_type: string
  limit_value: number
  window_secs: number
  window_tz: string | null
  window_type: string
  enabled: number | boolean
  priority: number
  provider_name_pattern: string | null
  skip_owner: number | boolean
  created_at: string
  updated_at: string
}

interface UsageRecord {
  rule_id: number
  subject_key: string
  model: string
  window_start: string
  counter: number
}

export default function Quota() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'org_admin'
  const [rules, setRules] = useState<QuotaRule[]>([])
  const [usage, setUsage] = useState<UsageRecord[]>([])
  const [groups, setGroups] = useState<string[]>([])
  const [userKeys, setUserKeys] = useState<{ id: number; name: string }[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [editingRule, setEditingRule] = useState<QuotaRule | null>(null)
  const [usageQuery, setUsageQuery] = useState({ subject: '', model: '' })
  const [form] = Form.useForm()

  const fetchUserKeys = async () => {
    try {
      const res = await listKeys()
      const keys = (res.data ?? []).map((k: any) => ({ id: k.id, name: k.name }))
      setUserKeys(keys)
    } catch { /* ignore */ }
  }

  const fetchRules = async () => {
    setLoading(true)
    try {
      const res = await listQuotaRules()
      setRules(Array.isArray(res.data) ? res.data : [])
    } finally {
      setLoading(false)
    }
  }

  const fetchUsage = async (params?: any) => {
    try {
      const res = await getQuotaUsage(params)
      setUsage(Array.isArray(res.data) ? res.data : [])
    } catch { /* ignore */ }
  }

  const fetchGroups = async () => {
    try {
      const res = await getQuotaGroups()
      setGroups(Array.isArray(res.data) ? res.data : [])
    } catch { /* ignore */ }
  }

  useEffect(() => {
    if (isAdmin) {
      fetchRules()
      fetchGroups()
    }
    fetchUserKeys()
  }, [])

  const handleCreate = () => {
    setEditingRule(null)
    setModalOpen(true)
  }

  const handleEdit = (record: QuotaRule) => {
    setEditingRule(record)
    setModalOpen(true)
  }

  const handleDelete = async (id: number) => {
    try {
      await deleteQuotaRule(id)
      message.success(t('quota.deleted'))
      fetchRules()
    } catch {
      message.error(t('quota.deleteFailed'))
    }
  }

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields()
      if (editingRule) {
        await updateQuotaRule(editingRule.id, values)
        message.success(t('quota.updated'))
      } else {
        await createQuotaRule(values)
        message.success(t('quota.created'))
      }
      setModalOpen(false)
      fetchRules()
      fetchGroups()
    } catch {
      message.error(t('quota.formError'))
    }
  }

  const handleInvalidateCache = async () => {
    try {
      await invalidateQuotaCache()
      message.success(t('quota.cacheInvalidated'))
    } catch {
      message.error(t('quota.cacheInvalidateFailed'))
    }
  }

  const handleSearchUsage = () => {
    const params: any = {}
    if (usageQuery.subject) params.subject = usageQuery.subject
    if (usageQuery.model) params.model = usageQuery.model
    fetchUsage(params)
  }

  const formatWindow = (secs: number, tz: string | null) => {
    if (secs === 0) return t('quota.lifetime')
    if (secs === 86400) return `${t('quota.daily')}${tz ? ` (${tz})` : ''}`
    if (secs === 3600) return t('quota.hourly')
    return `${secs}s${tz ? ` (${tz})` : ''}`
  }

  const ruleColumns: ColumnsType<QuotaRule> = [
    { title: 'ID', dataIndex: 'id', width: 50 },
    { title: t('quota.ruleName'), dataIndex: 'name', width: 140, ellipsis: true },
    {
      title: t('quota.quotaGroup'), dataIndex: 'quota_group', width: 110,
      render: (v) => v ? <Tag color="blue">{v}</Tag> : <Tag>global</Tag>,
    },
    { title: t('quota.dimension'), dataIndex: 'dimension', width: 100, render: (v) => <Tag>{v}</Tag> },
    { title: t('quota.modelPattern'), dataIndex: 'model_pattern', width: 110, ellipsis: true },
    {
      title: t('quota.limitType'), dataIndex: 'limit_type', width: 80,
      render: (v) => <Tag color={v === 'tokens' ? 'green' : 'orange'}>{v}</Tag>,
    },
    {
      title: t('quota.limitValue'), dataIndex: 'limit_value', width: 120,
      render: (v) => v.toLocaleString(),
    },
    {
      title: t('quota.window'), key: 'window', width: 130,
      render: (_, r) => formatWindow(r.window_secs, r.window_tz),
    },
    {
      title: t('quota.windowType'), dataIndex: 'window_type', width: 80,
      render: (v) => <Tag>{v}</Tag>,
    },
    {
      title: t('quota.enabled'), dataIndex: 'enabled', width: 60,
      render: (v) => <Tag color={v ? 'green' : 'default'}>{v ? t('common.yes') : t('common.no')}</Tag>,
    },
    {
      title: t('common.actions'), key: 'actions', width: 120, fixed: 'right',
      render: (_, record) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => handleEdit(record)}>{t('common.edit')}</Button>
          <Popconfirm title={t('quota.deleteConfirm')} onConfirm={() => handleDelete(record.id)}>
            <Button type="link" size="small" danger>{t('common.delete')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  const usageColumns: ColumnsType<UsageRecord> = [
    { title: 'Rule ID', dataIndex: 'rule_id', width: 80 },
    { title: t('quota.subject'), dataIndex: 'subject_key', width: 140 },
    { title: t('quota.model'), dataIndex: 'model', width: 140 },
    { title: t('quota.windowStart'), dataIndex: 'window_start', width: 180 },
    { title: t('quota.counter'), dataIndex: 'counter', width: 120, render: (v) => v.toLocaleString() },
  ]

  const usageTab = {
    key: 'usage',
    label: t('quota.usage'),
    children: (
      <Card>
        <Space style={{ marginBottom: 16 }} wrap>
          <Select
            placeholder={t('quota.selectKey')}
            allowClear
            style={{ width: 200 }}
            onChange={(val) => setUsageQuery({ ...usageQuery, subject: val || '' })}
            options={userKeys.map(k => ({ value: `token:${k.id}`, label: k.name }))}
          />
          <Input
            placeholder="model (e.g. * or gpt-4)"
            value={usageQuery.model}
            onChange={(e) => setUsageQuery({ ...usageQuery, model: e.target.value })}
            style={{ width: 200 }}
          />
          <Button type="primary" icon={<SearchOutlined />} onClick={handleSearchUsage}>
            {t('quota.search')}
          </Button>
        </Space>
        {usage.length > 0 ? (
          <Table
            columns={usageColumns}
            dataSource={usage}
            rowKey={(r) => `${r.rule_id}-${r.subject_key}-${r.model}-${r.window_start}`}
            pagination={false}
            size="small"
          />
        ) : (
          <Empty description={t('quota.noUsageData')} />
        )}
      </Card>
    ),
  }

  const tabItems = isAdmin ? [
    {
      key: 'rules',
      label: t('quota.rules'),
      children: (
        <Card>
          <Space style={{ marginBottom: 16 }}>
            <Button type="primary" icon={<PlusOutlined />} onClick={handleCreate}>
              {t('quota.addRule')}
            </Button>
            <Button icon={<ReloadOutlined />} onClick={handleInvalidateCache}>
              {t('quota.invalidateCache')}
            </Button>
          </Space>
          <Table
            columns={ruleColumns}
            dataSource={rules}
            rowKey="id"
            loading={loading}
            pagination={false}
            scroll={{ x: 1200 }}
            size="small"
          />
        </Card>
      ),
    },
    usageTab,
    {
      key: 'groups',
      label: t('quota.groups'),
      children: (
        <Card>
          <Descriptions column={1} bordered size="small">
            {groups.length > 0 ? groups.map((g) => (
              <Descriptions.Item key={g} label={t('quota.groupName')}>
                <Tag color="blue">{g}</Tag>
              </Descriptions.Item>
            )) : (
              <Descriptions.Item label="">{t('quota.noGroups')}</Descriptions.Item>
            )}
          </Descriptions>
        </Card>
      ),
    },
  ] : [usageTab]

  return (
    <div style={{ padding: 12 }}>
      <Tabs
        defaultActiveKey={isAdmin ? 'rules' : 'usage'}
        items={tabItems}
      />

      <Modal
        title={editingRule ? t('quota.editRule') : t('quota.addRule')}
        open={modalOpen}
        onOk={handleSubmit}
        onCancel={() => setModalOpen(false)}
        width={600}
        afterOpenChange={(open) => {
          if (open) {
            form.resetFields()
            if (editingRule) {
              form.setFieldsValue({
                name: editingRule.name,
                quota_group: editingRule.quota_group,
                dimension: editingRule.dimension,
                model_pattern: editingRule.model_pattern,
                limit_type: editingRule.limit_type,
                limit_value: editingRule.limit_value,
                window_secs: editingRule.window_secs,
                window_tz: editingRule.window_tz,
                window_type: editingRule.window_type,
                enabled: editingRule.enabled ? true : false,
                priority: editingRule.priority,
                provider_name_pattern: editingRule.provider_name_pattern,
                skip_owner: editingRule.skip_owner ? true : false,
              })
            } else {
              form.setFieldsValue({
                model_pattern: '*',
                limit_type: 'tokens',
                window_secs: 86400,
                window_tz: 'Asia/Shanghai',
                window_type: 'fixed',
                enabled: true,
                priority: 100,
                skip_owner: false,
              })
            }
          }
        }}
        destroyOnHidden
      >
        <Form form={form} layout="vertical" size="small">
          <Form.Item name="name" label={t('quota.ruleName')} rules={[{ required: true }]}>
            <Input placeholder="e.g. daily_30m" />
          </Form.Item>
          <Form.Item name="quota_group" label={t('quota.quotaGroup')}>
            <Input placeholder="e.g. standard" />
          </Form.Item>
          <Space style={{ display: 'flex' }} align="start">
            <Form.Item name="dimension" label={t('quota.dimension')} rules={[{ required: true }]} style={{ width: 160 }}>
              <Select>
                <Select.Option value="user">user</Select.Option>
                <Select.Option value="token">token</Select.Option>
                <Select.Option value="user_model">user_model</Select.Option>
                <Select.Option value="token_model">token_model</Select.Option>
                <Select.Option value="owner">owner</Select.Option>
              </Select>
            </Form.Item>
            <Form.Item name="model_pattern" label={t('quota.modelPattern')} style={{ width: 200 }}>
              <Input placeholder="* or gpt-4*" />
            </Form.Item>
          </Space>
          <Space style={{ display: 'flex' }} align="start">
            <Form.Item name="limit_type" label={t('quota.limitType')} rules={[{ required: true }]} style={{ width: 130 }}>
              <Select>
                <Select.Option value="tokens">tokens</Select.Option>
                <Select.Option value="requests">requests</Select.Option>
              </Select>
            </Form.Item>
            <Form.Item name="limit_value" label={t('quota.limitValue')} rules={[{ required: true }]} style={{ width: 200 }}>
              <InputNumber min={0} style={{ width: '100%' }} placeholder="30000000" />
            </Form.Item>
          </Space>
          <Space style={{ display: 'flex' }} align="start">
            <Form.Item name="window_secs" label={t('quota.windowSecs')} style={{ width: 130 }}>
              <InputNumber min={0} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="window_tz" label={t('quota.windowTz')} style={{ width: 160 }}>
              <Input placeholder="Asia/Shanghai" />
            </Form.Item>
            <Form.Item name="window_type" label={t('quota.windowType')} style={{ width: 110 }}>
              <Select>
                <Select.Option value="fixed">fixed</Select.Option>
                <Select.Option value="sliding">sliding</Select.Option>
              </Select>
            </Form.Item>
          </Space>
          <Space style={{ display: 'flex' }} align="start">
            <Form.Item name="priority" label={t('quota.priority')} style={{ width: 100 }}>
              <InputNumber min={0} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="enabled" label={t('quota.enabled')} style={{ width: 100 }}>
              <Select>
                <Select.Option value={true}>{t('common.yes')}</Select.Option>
                <Select.Option value={false}>{t('common.no')}</Select.Option>
              </Select>
            </Form.Item>
            <Form.Item name="skip_owner" label={t('quota.skipOwner')} style={{ width: 120 }}>
              <Select>
                <Select.Option value={false}>{t('common.no')}</Select.Option>
                <Select.Option value={true}>{t('common.yes')}</Select.Option>
              </Select>
            </Form.Item>
          </Space>
          <Form.Item name="provider_name_pattern" label={t('quota.providerPattern')}>
            <Input placeholder="* or openai*" allowClear />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
