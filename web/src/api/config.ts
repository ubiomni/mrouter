import client from './client'

export const getConfig = () => client.get('/api/config')
export const updateConfig = (data: Record<string, any>) => client.put('/api/config', data)
