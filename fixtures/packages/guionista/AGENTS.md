# Instrucciones base del proyecto

Este archivo lo lee el runtime de agentes de Narra al iniciar. Es el
contrato general para cualquier skill o agente que trabaje en este
proyecto. Las reglas creativas viven aquí y en `.narra/`; el motor de
Narra solo aporta lectura, propuestas revisables y formato.

## Idioma
Escribe siempre en español (salvo que se indique lo contrario para un
proyecto específico). Registro: el que corresponda al proyecto —
revisa `biblia/premisa.md` para tono y voz.

## Reglas de consistencia
- Antes de escribir o modificar cualquier escena, personaje o beat de
  estructura, lee `biblia/` completa (premisa, personajes, mundo,
  cronología). La biblia es la fuente de verdad del universo narrativo.
- Si vas a introducir un hecho nuevo (un lugar, una regla del mundo,
  un rasgo de personaje) que no está en la biblia, primero pregúntale
  al usuario si lo agregas a la biblia, o hazlo y avísale.
- Nunca contradigas la biblia sin señalarlo explícitamente como una
  propuesta de cambio.
- Todo cambio en archivos del proyecto se entrega como propuesta
  revisable; nunca se aplica sin que el usuario lo apruebe.

## Formato de guion
El formato de entrega es Fountain (ver skill `formato-fountain`).
Los archivos de guion viven en `guion/`, un archivo por acto,
episodio o bloque según convenga al proyecto.

## Modo de trabajo
- Por defecto, propone y pregunta antes de escribir grandes bloques
  (una escena completa, un acto entero). Los pasos pequeños
  (una idea, una lista, una ficha de personaje) puedes resolverlos
  directamente.
- El usuario puede pedir "modo autónomo" para una tarea puntual; en
  ese caso, avanza sin pedir aprobación en cada paso, pero resume al
  final qué se hizo y qué decisiones tomaste.
- Cuando actúes como editor/script editor (skill `editor-guion`), no
  reescribas: da notas. Reescribir es tarea del agente `guionista`,
  no del `editor`.

## Estructura del proyecto
```
AGENTS.md            este archivo
biblia/               personajes, mundo, premisa, cronología — la "fuente de verdad"
investigacion/        notas y fuentes de investigación
estructura/           método elegido, escaleta, tratamiento
guion/                archivos .fountain del guion
notas/                notas de desarrollo, notas del script editor
.narra/               agentes, comandos y skills de este proyecto
```
