import client from './client'

export const listKeys = () => client.get('/api/keys')
export const createKey = (data: any) => client.post('/api/keys', data)
export const updateKey = (id: number, data: any) => client.put(`/api/keys/${id}`, data)
export const deleteKey = (id: number) => client.delete(`/api/keys/${id}`)
