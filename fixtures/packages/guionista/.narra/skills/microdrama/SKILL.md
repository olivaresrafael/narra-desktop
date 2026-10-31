---
name: microdrama
description: Usa este skill para estructurar micro dramas web (episodios cortos, verticales, con gancho inicial y cliffhanger) — distinto del ritmo de largometraje o serie tradicional.
---

# Micro drama (formato web vertical)

## Objetivo
Estructurar historias en episodios muy cortos (típicamente 1-3 min),
pensados para consumo vertical/móvil, con retención episodio a
episodio como prioridad de diseño.

## Reglas de formato
- **Gancho en los primeros 3-5 segundos** de cada episodio — no hay
  tiempo de setup lento.
- **Un cliffhanger claro al final de cada episodio**, que empuje al
  siguiente. Señálalo explícito en la escaleta: `CLIFFHANGER: ...`.
- **Emociones e información grandes y legibles** — el formato no
  perdona sutileza excesiva; los giros deben ser claros incluso sin
  atención completa del espectador.
- **Pocos personajes por episodio** — normalmente 2-3 en escena.

## Proceso
1. Define el arco total en beats grandes (puede usar cualquier
   método de `metodos-estructura`, pero comprimido).
2. Divide el arco en episodios, cada uno con su propio gancho +
   cliffhanger, usando el formato de escena de `escaleta` más el
   campo `CLIFFHANGER`.
3. Guarda cada episodio como archivo separado en `guion/` (por
   ejemplo `ep01.fountain`, `ep02.fountain`).

## Qué NO hacer
- No apliques el ritmo de largometraje (setup extendido, escenas de
  transición) — cada episodio debe funcionar solo, y también como
  parte de la cadena.
