# 🤖 AI Agent Guide: Tachyon-Tex API 🚀

Este documento está diseñado para ser consumido por Agentes de IA que operan sobre este sistema de compilación LaTeX.

## 🧠 Filosofía del Sistema (Contexto para el Agente)
- **Ultra-Fast (Moonshot)**: La latencia es el enemigo. El sistema está diseñado para dar feedback en <1 segundo.
- **Efímero y Stateless**: Cada request vive en un disco RAM segregado. Nada persiste tras el cierre de la conexión.
- **Zero-I/O**: No asumas que el servidor tiene sistema de archivos persistente. Todo debe viajar en el request.

## 📡 Endpoints y Protocolos

### 1. `POST /compile` — El Motor de Renderizado
Es el endpoint principal. Convierte LaTeX a PDF.

**Capacidades Críticas:**
- **Soporte Multi-archivo**: Puedes enviar una lista de archivos `.tex`, `.bib`, `.sty`, `.cls` en un solo request `multipart/form-data`. No necesitas crear un `.zip` si envías archivos individuales.
- **Detección Inteligente**: El sistema busca automáticamente el archivo raíz (scanning por `\begin{document}`). No es obligatorio que se llame `main.tex`.

**Headers de Respuesta Útiles para Optimización:**
- `X-Compile-Time-Ms`: Úsalo para medir performance y reportar latencia al usuario.
- `X-Files-Received`: Verifica que todos los fragmentos de código que enviaste llegaron correctamente.

---

### 2. `POST /validate` — Pre-flight Check (Tu Herramienta de Linter)
**¡IMPORTANTE!** Usa este endpoint **antes** de compilar para ahorrar recursos y tiempo.

**Qué detecta:**
- Estructura básica (`\documentclass`, `\begin{document}`).
- **Llaves desbalanceadas**: Muy común en fragmentos generados por IA.
- **Mismatch de entornos**: Si abres un `\begin{itemize}` y lo cierras con un `\end{enumerate}`.
- **Comandos obsoletos**: Sugiere cambios de `\bf` a `\textbf{}`.

---

### 3. `GET /packages` — Inventario de Capacidades
Consulta este endpoint para saber qué paquetes básicos están pre-cargados en cache. Si un paquete no está aquí, Tectonic intentará descargarlo al vuelo (lo cual añade latencia de red).

## 🛠️ Workflow de Instrucción (Cómo debe actuar un Agente)

1. **Paso 1 (Validación Local)**: Genera el código LaTeX.
2. **Paso 2 (Escaneo de Dependencias)**: Comenta al usuario si usas paquetes pesados que no estén en `/packages`.
3. **Paso 3 (Audit)**: Llama a `/validate`. Si `valid: false`, corrige el LaTeX antes de intentar la compilación pesada.
4. **Paso 4 (Execution)**: Llama a `/compile`. Si falla, analiza el `stderr` devuelto en el cuerpo de la respuesta para el "Self-Correction Loop".

## ⚠️ Consideraciones de Seguridad y Performance
- **Timeout**: El motor tiene un timeout interno. Evita loops infinitos de TikZ o macros recursivas.
- **Multipart**: Cuando envíes múltiples archivos, asegúrate de asignar nombres de archivo coherentes (ej: `main.tex`, `ref.bib`) en el campo `filename` de cada parte del form-data.
- **RAM Limit**: El disco RAM es de 512MB. Evita subir imágenes de altísima resolución si no son necesarias.

---
**System ID**: `Tachyon-Tex-v1.2-Optimized`
**Engine**: `Tectonic (Rust)`
