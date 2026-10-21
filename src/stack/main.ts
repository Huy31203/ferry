import { mount } from 'svelte';
import '../lib/theme.css';
import { applyOsClass } from '../lib/ipc';
import Stack from './Stack.svelte';

applyOsClass();
mount(Stack, { target: document.getElementById('app')! });
