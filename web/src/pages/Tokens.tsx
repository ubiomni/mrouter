import { useEffect, useState } from 'react'
import {
  Table, Button, Space, Modal, Form, Input, Tag, Popconfirm, Typography, DatePicker, App, Switch,
} from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import dayjs from 'dayjs'
import { listTokens, createToken, updateToken, deleteToken, regenerateToken } from '@/api/tokens'

const { Paragraph } = Typography

interface Token {
  id: number
  name: string
  token: string
  is_active: boolean
  expires_at: number | null
  created_at: number
  last_used_at: number | null
}

export default function Tokens() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [data, setData] = useState<Token[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [createdTokenValue, setCreatedTokenValue] = useState<string | null>(null)
  const [form] = Form.useForm()

  const fetchData = async () => {
    setLoading(true)
    try {
      const res = await listTokens()
      setData(res.data ?? [])
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { fetchData() }, [])

  const handleCreate = () => {
    form.resetFields()
    setModalOpen(true)
  }

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields()
      const payload: any = { name: values.name }
      if (values.expires_at) {
        payload.expires_at = String(values.expires_at.unix())
      }
      const res = await createToken(payload)
      setCreatedTokenValue(res.data?.token)
      message.success(t('tokens.created'))
      setModalOpen(false)
      fetchData()
    } catch {
      message.error(t('tokens.createFailed'))
    }
  }

  const handleToggle = async (record: Token, active: boolean) => {
    try {
      await updateToken(record.id, { is_active: active })
      message.success(t('tokens.updated'))
      fetchData()
    } catch {
      message.error(t('common.failed'))
    }
  }

  const handleRegenerate = async (id: number) => {
    try {
      const res = await regenerateToken(id)
      setCreatedTokenValue(res.data?.token)
      message.success(t('tokens.regenerated'))
      fetchData()
    } catch {
      message.error(t('common.failed'))
    }
  }

  const handleDelete = async (id: number) => {
    try {
      await deleteToken(id)
      message.success(t('tokens.deleted'))
      fetchData()
    } catch {
      message.error(t('tokens.deleteFailed'))
    }
  }

  const columns: ColumnsType<Token> = [
    { title: t('common.id'), dataIndex: 'id', key: 'id', width: 60 },
    { title: t('common.name'), dataIndex: 'name', key: 'name' },
    {
      title: 'Token', dataIndex: 'token', key: 'token',
      render: (v) => <code>{v}...</code>,
    },
    {
      title: t('common.status'), dataIndex: 'is_active', key: 'is_active', width: 100,
      render: (v, record) => (
        <Switch size="small" checked={v} onChange={(checked) => handleToggle(record, checked)} />
      ),
    },
    {
      title: t('tokens.expiresAt'), dataIndex: 'expires_at', key: 'expires_at',
      render: (v) => v ? dayjs.unix(v).format('YYYY-MM-DD HH:mm') : t('common.never'),
    },
    {
      title: t('tokens.lastUsed'), dataIndex: 'last_used_at', key: 'last_used_at',
      render: (v) => v ? dayjs.unix(v).format('YYYY-MM-DD HH:mm') : t('common.never'),
    },
    {
      title: t('common.createdAt'), dataIndex: 'created_at', key: 'created_at',
      render: (v) => v ? dayjs.unix(v).format('YYYY-MM-DD HH:mm') : '-',
    },
    {
      title: t('common.actions'), key: 'actions', width: 200,
      render: (_, record) => (
        <Space size="small">
          <Popconfirm title={t('tokens.regenerateConfirm')} onConfirm={() => handleRegenerate(record.id)} okText={t('common.yes')} cancelText={t('common.no')}>
            <Button type="link" size="small">{t('tokens.regenerate')}</Button>
          </Popconfirm>
          <Popconfirm title={t('tokens.deleteConfirm')} onConfirm={() => handleDelete(record.id)} okText={t('common.yes')} cancelText={t('common.no')}>
            <Button type="link" size="small" danger>{t('common.delete')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div style={{ padding: 12 }}>
      <Space style={{ marginBottom: 16 }}>
        <Button type="primary" onClick={handleCreate}>{t('tokens.add')}</Button>
      </Space>

      <Table columns={columns} dataSource={data} rowKey="id" loading={loading} pagination={false} size="middle" />

      <Modal
        title={t('tokens.create')}
        open={modalOpen}
        onOk={handleSubmit}
        onCancel={() => setModalOpen(false)}
        destroyOnHidden
      >
        <Form form={form} layout="vertical">
          <Form.Item name="name" label={t('common.name')} rules={[{ required: true, message: t('tokens.nameRequired') }]}>
            <Input />
          </Form.Item>
          <Form.Item name="expires_at" label={t('tokens.expiresAt')}>
            <DatePicker style={{ width: '100%' }} showTime />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title={t('tokens.createdTitle')}
        open={!!createdTokenValue}
        onOk={() => setCreatedTokenValue(null)}
        onCancel={() => setCreatedTokenValue(null)}
        cancelButtonProps={{ style: { display: 'none' } }}
      >
        <p style={{ marginBottom: 8, color: '#ff4d4f', fontWeight: 500 }}>
          {t('tokens.createdWarning')}
        </p>
        <Paragraph copyable={{ text: createdTokenValue ?? '' }} style={{ fontFamily: 'monospace' }}>
          {createdTokenValue}
        </Paragraph>
      </Modal>
    </div>
  )
}
