import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Onboarding from './Onboarding.svelte';

applyOsClass();
void applyAccent();
mount(Onboarding, { target: document.getElementById('app')! });
