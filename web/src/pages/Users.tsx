import { useEffect, useState } from 'react'
import {
  Table, Button, Space, Modal, Form, Input, Select, Tag, Popconfirm, App,
} from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import dayjs from 'dayjs'
import { listUsers, createUser, updateUser, deleteUser } from '@/api/users'

interface User {
  id: number
  username: string
  display_name: string
  email: string
  role: string
  is_active: boolean
  last_login: string | null
  created_at: string
}

export default function Users() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [data, setData] = useState<User[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [editingUser, setEditingUser] = useState<User | null>(null)
  const [form] = Form.useForm()

  const fetchData = async () => {
    setLoading(true)
    try {
      const res = await listUsers()
      setData(res.data ?? [])
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { fetchData() }, [])

  const handleCreate = () => {
    setEditingUser(null)
    form.resetFields()
    form.setFieldsValue({ role: 'user', is_active: true })
    setModalOpen(true)
  }

  const handleEdit = (record: User) => {
    setEditingUser(record)
    form.setFieldsValue({
      username: record.username,
      display_name: record.display_name,
      email: record.email,
      role: record.role,
      is_active: record.is_active,
    })
    setModalOpen(true)
  }

  const handleDelete = async (id: number) => {
    try {
      await deleteUser(id)
      message.success(t('users.deleted'))
      fetchData()
    } catch {
      message.error(t('users.deleteFailed'))
    }
  }

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields()
      if (editingUser) {
        await updateUser(editingUser.id, values)
        message.success(t('users.updated'))
      } else {
        await createUser(values)
        message.success(t('users.created'))
      }
      setModalOpen(false)
      fetchData()
    } catch {
      message.error(t('users.formError'))
    }
  }

  const columns: ColumnsType<User> = [
    { title: t('common.id'), dataIndex: 'id', key: 'id', width: 60 },
    { title: t('users.username'), dataIndex: 'username', key: 'username' },
    { title: t('users.displayName'), dataIndex: 'display_name', key: 'display_name', render: (v) => v || '-' },
    {
      title: t('users.role'), dataIndex: 'role', key: 'role',
      render: (v) => {
        const color = v === 'super_admin' ? 'purple' : v === 'org_admin' ? 'red' : 'blue';
        return <Tag color={color}>{v}</Tag>;
      },
    },
    {
      title: t('users.active'), dataIndex: 'is_active', key: 'is_active',
      render: (v) => <Tag color={v ? 'green' : 'default'}>{v ? t('common.yes') : t('common.no')}</Tag>,
    },
    {
      title: t('users.lastLogin'), dataIndex: 'last_login', key: 'last_login',
      render: (v) => (v ? dayjs(v).format('YYYY-MM-DD HH:mm') : t('common.never')),
    },
    {
      title: t('common.actions'), key: 'actions', width: 150,
      render: (_, record) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => handleEdit(record)}>{t('common.edit')}</Button>
          <Popconfirm title={t('users.deleteConfirm')} onConfirm={() => handleDelete(record.id)} okText={t('common.yes')} cancelText={t('common.no')}>
            <Button type="link" size="small" danger>{t('common.delete')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div style={{ padding: 12 }}>
      <Space style={{ marginBottom: 16 }}>
        <Button type="primary" onClick={handleCreate}>{t('users.add')}</Button>
      </Space>

      <Table columns={columns} dataSource={data} rowKey="id" loading={loading} pagination={false} />

      <Modal
        title={editingUser ? t('users.edit') : t('users.create')}
        open={modalOpen}
        onOk={handleSubmit}
        onCancel={() => setModalOpen(false)}
        destroyOnHidden
      >
        <Form form={form} layout="vertical">
          <Form.Item name="username" label={t('users.username')} rules={[{ required: true, message: t('users.usernameRequired') }]}>
            <Input disabled={!!editingUser} />
          </Form.Item>
          {!editingUser && (
            <Form.Item name="password" label={t('users.password')} rules={[{ required: true, message: t('users.passwordRequired') }]}>
              <Input.Password />
            </Form.Item>
          )}
          {editingUser && (
            <Form.Item name="password" label={t('users.resetPassword')} extra={t('users.resetPasswordHint')}>
              <Input.Password />
            </Form.Item>
          )}
          <Form.Item name="display_name" label={t('users.displayName')}><Input /></Form.Item>
          <Form.Item name="email" label={t('users.email')}><Input /></Form.Item>
          <Form.Item name="role" label={t('users.role')} rules={[{ required: true }]}>
            <Select>
              <Select.Option value="super_admin">Super Admin</Select.Option>
              <Select.Option value="org_admin">Org Admin</Select.Option>
              <Select.Option value="user">User</Select.Option>
            </Select>
          </Form.Item>
          <Form.Item name="is_active" label={t('users.active')}>
            <Select>
              <Select.Option value={true}>{t('common.active')}</Select.Option>
              <Select.Option value={false}>{t('common.inactive')}</Select.Option>
            </Select>
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
