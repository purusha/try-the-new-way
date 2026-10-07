// Accesso a localStorage protetto: può non esistere o lanciare eccezioni
// (navigazione privata, dati del sito bloccati). In quel caso si usa la memoria.

const memory = new Map<string, string>()

export function readStored(key: string): string {
  try {
    const value = window.localStorage.getItem(key)
    if (value !== null) return value
  } catch {
    // localStorage non disponibile
  }
  return memory.get(key) ?? ''
}

export function writeStored(key: string, value: string): void {
  memory.set(key, value)
  try {
    if (value) window.localStorage.setItem(key, value)
    else window.localStorage.removeItem(key)
  } catch {
    // localStorage non disponibile: resta il valore in memoria
  }
}
