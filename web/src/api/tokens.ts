import client from './client'

export const listTokens = () => client.get('/api/tokens')
export const getToken = (id: number) => client.get(`/api/tokens/${id}`)
export const createToken = (data: { name: string; expires_at?: string }) =>
  client.post('/api/tokens', data)
export const updateToken = (id: number, data: Record<string, any>) =>
  client.put(`/api/tokens/${id}`, data)
export const deleteToken = (id: number) => client.delete(`/api/tokens/${id}`)
export const regenerateToken = (id: number) => client.post(`/api/tokens/${id}/regenerate`)
export const getTokenStats = (id: number, from?: string, to?: string) =>
  client.get(`/api/tokens/${id}/stats`, { params: { from, to } })
export const getAllTokenStats = (from?: string, to?: string) =>
  client.get('/api/tokens/stats', { params: { from, to } })
