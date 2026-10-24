import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Batch from './Batch.svelte';

applyOsClass();
void applyAccent();
mount(Batch, { target: document.getElementById('app')! });
