import client from './client'

export const listLogs = (params: {
  page?: number
  size?: number
  provider_id?: number
  model?: string
  status?: string
  token_name?: string
  from?: string
  to?: string
}) => client.get('/api/logs', { params })
