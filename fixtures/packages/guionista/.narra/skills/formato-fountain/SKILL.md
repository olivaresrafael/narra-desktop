---
name: formato-fountain
description: Reglas de formato Fountain para que todo el guion salga consistente y sea compatible con lectores/conversores estándar (incluida su importación a Celtx u otros programas).
---

# Formato Fountain

## Qué es
Fountain es texto plano con convenciones simples que cualquier
programa de guion reconoce (encabezados de escena, personajes,
diálogo, acción, transiciones). Es la fuente de verdad de este kit;
otros formatos (PDF, .fdx, Celtx) se exportan desde aquí.

## Reglas clave
- **Encabezado de escena:** `INT. LUGAR - DÍA` o `EXT. LUGAR - NOCHE`
  en mayúsculas, sin numerar (la numeración se agrega al exportar).
- **Personaje:** nombre en mayúsculas, solo en su propia línea, antes
  del diálogo.
- **Paréntesis (acotación de diálogo):** línea aparte, entre
  paréntesis, justo después del nombre del personaje o dentro del
  diálogo si es una pausa breve. Úsalo con moderación.
- **Acción:** texto normal, tiempo presente, sin sangría especial.
- **Transiciones:** en mayúsculas, alineadas a la derecha en la
  exportación final (`CORTE A:`, `FUNDE A NEGRO:`) — en el archivo
  Fountain solo se escriben en mayúsculas seguidas de dos puntos.
- **Notas de producción o del guionista:** `[[así, entre doble
  corchete]]` — no se exportan al PDF final salvo que se pida.
- **Título:** bloque al inicio del archivo con metadata:
  ```
  Title: <Título>
  Credit: Escrito por
  Author: <Nombre>
  Draft date: <fecha>
  ```

## Verificación
Antes de dar por buena una escena, revisa:
- ¿Cada encabezado de escena tiene INT/EXT, lugar y DÍA/NOCHE?
- ¿Los nombres de personaje están en mayúsculas y coinciden
  exactamente con `biblia/personajes/` (mismo nombre siempre)?
- ¿No hay diálogo sin nombre de personaje encima?

## Qué NO hacer
- No mezcles formato de guion de cine con formato de obra de teatro
  o novela dentro del mismo archivo.
- No renombres a un personaje a mitad de guion (ej. "CARLOS" en una
  escena y "EL PADRE" en otra) sin que sea una decisión de guion
  deliberada y señalada.
